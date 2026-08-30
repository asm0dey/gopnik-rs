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
//! Address convention used by every citation below: `docs/re/METHODOLOGY.md`,
//! "Address convention, and its range of validity", is the authority, and
//! `tools/addr.py` is its executable form -- `python3 tools/re_query.py
//! resolve <citation>` converts one and prints the bytes there. A `mov di,<n>`
//! / `push cs` string operand at `1000:XXXX` names the string whose file
//! offset is what `1000:<n>` resolves to.
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
//! is checkpoint-only, at exactly two sites -- the mage's paid save
//! (`1000:761d`, `district * 50` rubles) and the district-advance autosave
//! (`0x9bcd`'s prompt, `1000:acc8`'s write). Neither is a typed verb.
//! `crate::persist` holds both, with the disassembly.
//!
//! [`Game::mage`] reaches the first of them and
//! [`Game::district_advance`] the second: its `ReadLn` sits at the top of
//! the original's main loop (`1000:ab75`..`1000:ad12`), which is where
//! [`Game::run`] runs it too. Task 21 moved the promotion there out of the
//! post-fight block; see `docs/re/gaps.md`, "The district-advance autosave
//! — wired (Task 21)".

use crate::character_sheet;
use crate::combat::{blows_per_round, resolve_blow_nth, Break, Swing};
use crate::combat_dispatch::{self, Backup, Called, Shot, Status};
use crate::commands::{parse, Command};
use crate::data;
use crate::locations::{Location, Places};
use crate::model::Fighter;
use crate::progress::{self, Progress};
use crate::rng::Rng;
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

/// One priced menu row whose price is an **instruction immediate**, not a
/// byte of the `20ae:0b2e` price array `data/shops.json` records.
///
/// The vet, the club and the gym build their rows with the same fixed
/// instruction shape the market rows use, with one substitution: the
/// affordability test is `cmp word [20ae:38c7],imm8` against a literal
/// instead of `mov al,[20ae:0bNN] / xor ah,ah / cmp ax,[20ae:38c7]`. That is
/// why `tools/extract_tables.py`'s price-array scan never saw them and why
/// `data/shops.json` carries only `mar` and `bmar`.
///
/// `site` is the address of that `cmp`, i.e. where `price` is written down in
/// the image. `prefix` and `text` are the two shortstrings the row is
/// assembled from, in that order, with the affordability colour digit
/// between them -- the same three-part shape [`Game::print_priced_rows`]
/// uses. Both are quoted verbatim, markup included.
///
/// `tools/difftest.py` re-derives this whole table out of `orig/g.exe` by
/// scanning for that instruction shape (nine hits, no more) and compares it
/// against what the port emits; `docs/re/difftest.md` has the enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImmRow {
    /// The verb whose handler contains the row: `rep`, `kl` or `trn`.
    pub shop: &'static str,
    /// The key the player types, read off the row's own prefix string.
    pub key: &'static str,
    /// The immediate at `site`, in rubles.
    pub price: i32,
    /// Address of the `cmp word [20ae:38c7],imm8` that carries `price`.
    pub site: &'static str,
    /// The prefix shortstring, ending in the bare `^` the colour digit
    /// completes.
    pub prefix: &'static str,
    /// The row's own shortstring. For these nine rows the price is part of
    /// the text as literal digits, not a `#` placeholder.
    pub text: &'static str,
}

/// Every immediate-priced menu row in the image, in address order.
///
/// Nine rows: two for the vet (`1000:d410`, `1000:d465`), two for the club
/// (`1000:df6f`, `1000:dfcb`) and five for the gym (`1000:e400`,
/// `1000:e455`, `1000:e4c4`, `1000:e521`, `1000:e58f`). Which handler a row
/// belongs to is decided by the verb-dispatch span it falls in: `rep`'s
/// token compare is at `1000:d3a6`, `girl`'s at `1000:d6ed`, `kl`'s at
/// `1000:df06`, `trn`'s at `1000:e390` and `kos`'s at `1000:e973`, each with
/// its own token string pushed five bytes earlier
/// (`docs/re/command-dispatch.md`).
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

/// What a replay of a captured fight needs that the draw stream cannot show.
///
/// Populated only while [`Game::start_fight_log`] is in force. The two lists
/// mirror `data/combat_trace.json`'s two fight channels exactly, marker for
/// marker: `fights` is one entry per `1000:3d11` stop and `prompts` one per
/// `1000:441d` stop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FightLog {
    /// The opponent as `Game::run_combat` received it, before any blow --
    /// the guest's `20ae:3952`.. record at the combat function's prologue --
    /// paired with the number of draws already spent when the fight started.
    pub fights: Vec<(usize, Fighter)>,
    /// One entry per `^0Битва\` prompt, in order.
    pub prompts: Vec<PromptState>,
}

/// Both fighters at one `^0Битва\` prompt: what the previous round left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptState {
    /// 1-based index of the fight this prompt belongs to.
    pub fight: usize,
    /// Draws made before this prompt, from [`crate::rng::Rng::draws_logged`].
    /// It is what ties this channel to the draw stream: a prompt recorded at
    /// the wrong point fails even when both channels are individually right.
    pub draws_before: usize,
    pub player_hp: u16,
    pub player_hpmax: u16,
    pub enemy_hp: u16,
    pub enemy_hpmax: u16,
    pub player_broken_jaw: bool,
    pub player_broken_leg: bool,
    pub enemy_broken_jaw: bool,
    pub enemy_broken_leg: bool,
}

pub struct Game {
    pub player: Fighter,
    pub progress: Progress,
    pub places: Places,
    pub district: u8,
    pub rng: Rng,
    pub location: Location,
    /// `20ae:38bb` -- the player owns a mobile phone. Gates draws 3 and 4 of
    /// the wander preamble and every phone-call message in it.
    pub has_mobile: bool,
    /// `20ae:3693` -- the flag wander bucket 1 toggles at `1000:b3c4`
    /// (`80 3e 93 36 00` / `b0 00` / `75 01` / `40` / `a2 93 36`: a plain
    /// boolean flip, read then written back inverted).
    ///
    /// **It is not flavour.** `FUN_1000_0d14` reads it twice -- at
    /// `1000:0d86` to decide whether to spend an extra `Random(4)` on the
    /// opponent's class (`1000:0d91`), and at `1000:0e54` to multiply the
    /// opponent's level by 1.5 (`1000:0e6c`). So the toggle changes both the
    /// draw count and the draw values of every later encounter, which is why
    /// this port has to carry it even though bucket 1 itself prints only
    /// flavour text.
    pub flag_3693: bool,
    /// `20ae:38b3` / `.SAV 0x217` -- тёмные очки, listed in the stat block by
    /// `1000:1cf8`/`1000:1cff` (`^1У тебя есть тёмные очки`). On the cop
    /// encounter's losing roll they are what stops the fight
    /// (`1000:b7c6` `cmp byte [0x38b3],1`).
    pub dark_glasses: bool,
    /// `20ae:38bc` / `.SAV 0x220` -- зоновская наколка, listed by
    /// `1000:1d18`/`1000:1d1f` (`^1На тебе зоновская наколка`) and bought at
    /// `1000:cb05` (`^2Чистый зек.`). It **halves** the ordinary encounter's
    /// notice roll at `1000:b5da`..`1000:b5ea`, and only that one: the cop
    /// encounter's roll at `1000:b784` has no such branch.
    pub prison_tattoo: bool,
    /// `20ae:38bf` / `.SAV 0x223` -- the first one-shot gift, granted by the
    /// church's `Random(5) == 2` arm (`1000:8134`) as well as the post-kill
    /// block `docs/re/progression.md` documents.
    pub oneshot_gift_1: bool,
    /// `20ae:38c0` / `.SAV 0x224` -- the second one-shot gift (`1000:8184`).
    pub oneshot_gift_2: bool,
    /// `20ae:38c1` / `.SAV 0x225` -- the ring "Господи помилуй"
    /// (`1000:81c4`). Gates the wander's +3 HP regen and draw 9.
    pub ring_gospodi_pomilui: bool,
    /// `20ae:38cb` / `.SAV 0x22f` -- понтовость на улице, the street-cred
    /// counter that is **not** the level at `20ae:38a6`. Gates draw 2's
    /// message (`>= 100`) and is topped up by the church's arm 4.
    pub pontovost_street: i32,
    /// `20ae:38cd` / `.SAV 0x231` -- the joint buff's countdown. Decremented
    /// at the top of every walk (`1000:aea8`); reaching zero takes the buff
    /// back. `crate::model::Fighter::stoned` is the same event as a bool;
    /// this is the counter the original actually keeps.
    pub buff_countdown: u8,
    /// `20ae:3b74` -- the theft amount -- is a global in the original, but it
    /// needs no field here, because **every** reader writes it immediately
    /// before reading it: no path carries a value in it across a turn
    /// boundary, so it is a local at each of its two use sites.
    ///
    /// **Established from flow**, both blocks re-derived from `orig/g.exe`
    /// from an aligned instruction start:
    ///
    /// * `1000:b313`..`1000:b346`, wander draw 11. `1000:b321` is the
    ///   `Random(district * 5)`; `1000:b326` `inc ax`, `1000:b327`
    ///   `a3 74 3b` stores, `1000:b32a` `a1 74 3b` reads back for
    ///   `1000:b32d` `add [0x38c7],ax` (money), and `1000:b336`
    ///   `ff 36 74 3b` pushes it into the message written at `1000:b346`.
    /// * `1000:c333`..`1000:c396`, a **second** pickpocket block in the
    ///   market, entered from the `0f78:0bd8` token compare at `1000:c329`
    ///   (`jz 0xc333`). Same shape, different gates: `Random(district*5 + 5)`
    ///   at `1000:c344` checked against luck `[0x38a4]` (`1000:c353`..
    ///   `1000:c35b`), then `Random(10)` at `1000:c361` with `cmp ax,9` /
    ///   `jnc 0xc3cd` at `1000:c366`, then `Random(luck * 2)` at
    ///   `1000:c371`; `1000:c376` `inc ax`, `1000:c377` `a3 74 3b` stores,
    ///   `1000:c37a` `a1 74 3b` reads back for `1000:c37d`
    ///   `add [0x38c7],ax`, `1000:c386` `ff 36 74 3b` pushes it.
    ///
    /// An earlier revision of this comment said "nothing outside
    /// `1000:b321`..`1000:b346` reads it". That was **false**, and it is the
    /// "scan whose completeness claim stopped the next search" failure
    /// `docs/re/METHODOLOGY.md` exists to stop. Scanning the whole image for
    /// the operand bytes `74 3b` returns **7** hits: `1000:b328`,
    /// `1000:b32b`, `1000:b338` (first block), `1000:c378`, `1000:c37b`,
    /// `1000:c388` (second block) -- and `1000:c358`, which is *not* an
    /// operand at all but the straddle of `1000:c357` `7c 74` (`jl 0xc3cd`)
    /// and `1000:c359` `3b c1` (`cmp ax,cx`). Six real references, two
    /// blocks. The *conclusion* above survives; the evidence first given for
    /// it did not.
    ///
    /// The second block's **three** draws (`1000:c344`, `1000:c361`,
    /// `1000:c371` -- the three enumerated above) are **not modelled** by this
    /// port -- see `docs/re/gaps.md`, "Opened by Task 11c".
    ///
    /// `20ae:3b76` -- the market ban's countdown, set to 5 at `1000:c465`
    /// (`c6 06 76 3b 05`), gated on at `1000:b95e`, cleared by `girl` at
    /// `1000:d793`, and decremented once per walk at `1000:b173`.
    ///
    /// **Only the decrement is implemented.** Nothing in this port assigns a
    /// non-zero value, so the field is permanently 0 here and the two things
    /// that read it -- the `== 1` phone message in [`Game::walk`] and the
    /// decrement itself -- never fire. Registered in `docs/re/gaps.md`, "The
    /// two ban countdowns are modelled and decremented but never set", with
    /// every missing site's address.
    pub market_ban_countdown: u8,
    /// `20ae:3b77` -- the club ban's countdown: set to 5 at `1000:e23e`
    /// (`c6 06 77 3b 05`), gated on at `1000:df1a`, decremented at
    /// `1000:b17e`. Same state as [`Game::market_ban_countdown`] -- only the
    /// decrement is implemented; see the same `docs/re/gaps.md` entry.
    pub club_ban_countdown: u8,
    /// `20ae:3b78` -- den errand one. Set by draw 1 at `1000:af71`, and set
    /// there **unconditionally**, before the flags that decide whether
    /// anything prints.
    pub den_errand_1_pending: bool,
    /// `20ae:3b79` -- den errand two (`1000:afd0`, same shape).
    pub den_errand_2_pending: bool,
    /// `20ae:3b72` -- the fight-accepted flag. Eight of its nine accepted
    /// references are stores and exactly one is a load, `1000:b81f`
    /// (`data/den_arms.json`'s `globals[]` census, recomputed by
    /// `python3 tools/re_query.py xrefs-to 20ae:3b72`).
    ///
    /// **The den's `hp` arm is the only writer this port carries.**
    /// `1000:dc11` `mov byte [0x3b72],0x1` is ported in
    /// [`Game::den_beat_up`]. The wander's own seven stores
    /// (`1000:b5bb`, `1000:b698`, `1000:b71a`, `1000:b747`, `1000:b81a`,
    /// and `1000:c3d3` / `1000:e184` outside it) are modelled by
    /// [`Game::walk`] as CONTROL FLOW -- it calls `run_combat` where the
    /// original sets the flag and lets `1000:b81f` read it -- so nothing in
    /// this port reads this field. Carried anyway, per the brief's "add
    /// whatever state the arms need ... on `Game` when it is a standalone
    /// global", and registered in `docs/re/gaps.md`, "The den's `hp` arm
    /// sets `20ae:3b72` and nothing in this port reads it".
    pub fight_accepted_3b72: bool,
    /// `20ae:394d` / `.SAV 0x2b1`, `20ae:394e`, `20ae:394f` -- the pistol, its
    /// silencer and its magazine. See [`crate::combat_dispatch::Pistol`],
    /// which carries the evidence for all three.
    ///
    /// This field used to be `dealer_order_placed: bool`, documented as "a
    /// 150-rouble order placed with the dealers (`1000:cd05`)". The address
    /// and the price were right; the reading was not. `1000:cd05`'s arm is
    /// `bmar` row 7 and it hands over the pistol -- `mov byte [0x394d],1`
    /// followed immediately by `1000:cd0a` `add word [0x394f],3` -- and
    /// `1000:cd7b` refuses row 8 without it with `^6Нету пушки. Сначала купи
    /// пистолет` (CS `0x9666`).
    pub pistol: crate::combat_dispatch::Pistol,
    /// `20ae:3e32` -- counts walks 0..25 once the PISTOL is owned
    /// (`1000:af24` `cmp byte [0x394d],0` is the gate on the increment), and
    /// the phone call fires at exactly 25 (`1000:af36`).
    ///
    /// What it is counting down to is the **silencer**: `1000:ce00`
    /// `cmp byte [0x3e32],0x19` is the only other reader, and it is `bmar`
    /// row 9's gate. So the counter is the dealers' delivery time on the one
    /// item they have to order in.
    pub dealer_delivery_counter: u8,
    /// `20ae:3c83` -- the rector showdown. **Confirmed** in Task 17
    /// (`docs/re/combat-dispatch.md`): six references image-wide, two writes
    /// and four reads, and **nothing ever clears it**. Its three effects are
    /// all in `FUN_1000_3d11`: no crowd (`1000:411d`), no fleeing
    /// (`1000:48eb`) and a death message that names the killer
    /// (`1000:4f8c`).
    ///
    /// **Both writers are DIFFERENT original addresses, not the same store
    /// reached twice.** `1000:ae13` is the per-turn one, inside the chapter-5
    /// endgame arm at the top of the main loop -- ported as
    /// [`Game::enter_district_5`], called from [`Game::district_advance`] on
    /// the turn `self.district` first becomes 5 during play.
    /// `1000:7364` is the entry-time one, inside
    /// `FUN_1000_6a0d` (the character-setup procedure, called exactly once,
    /// at `1000:ab72`, before the main loop's first iteration) -- it reads
    /// `[0x3692]`, the DISTRICT, at `1000:7262`/`1000:7347`, **not**
    /// `[0x389c]`, the class (an earlier revision of this doc called it a
    /// "class-5 character-creation arm" and invented a "set twice for a
    /// Гопник" story from that wrong reading; there is no class dispatch
    /// here at all). Because `FUN_1000_6a0d` runs on every entry into the
    /// game, new character OR loaded save (`docs/re/wander.md`, "What
    /// reaches `1000:73bb`"), a save loaded already at district 5 arms this
    /// flag before turn one. Ported as part of
    /// [`Game::apply_class_bonus`] (Task 20's review fix; see that method's
    /// doc for the full re-derivation) rather than a separate method,
    /// because `apply_class_bonus` is already the port's home for
    /// everything else this same original function re-applies on load.
    ///
    /// All three effects were already implemented before Task 20 and are
    /// now reachable in real play from both writers, not only from a test
    /// that sets the field directly. Same shape as
    /// [`Game::market_ban_countdown`]; registered in `docs/re/gaps.md`.
    pub rector_showdown: bool,
    /// `20ae:3e35` -- the den's loan credit. Set to 5 at `1000:73e5` and
    /// topped up once per walk while below `district * 10` (`1000:af19`).
    pub den_loan_credit: u8,
    /// `20ae:394a` / `.SAV 0x2ae` -- зубная защита. The ONLY thing it
    /// changes is a jaw break landing on the player: `1000:47e8`
    /// `cmp byte [0x394a],0` splits the break into the plain arm
    /// (`1000:47ee` sets the jaw) and a `Random(4)` at `1000:47fe` whose 0
    /// breaks it anyway and whose 1..3 does not. It is therefore a **draw
    /// count** difference, not just flavour: `docs/re/combat.md` listed it as
    /// unmodelled gap 3, and a save that ships it (`SAVE_R3`, `SAVE_R4`,
    /// `SAVE_R5` all hold 1 at `.SAV 0x2ae`) desynchronises any replay
    /// without it.
    pub tooth_guard: bool,
    /// `20ae:38bd` / `.SAV 0x221` -- the крестик, `luck += 2`, granted once
    /// by the post-kill item table (`1000:548c` gate, `1000:54b1` flag).
    pub charm_krestik_38bd: bool,
    /// `20ae:38be` / `.SAV 0x222` -- кольцо "Господи спаси", `luck += 1`
    /// (`1000:54bd` gate, `1000:54e1` flag).
    pub charm_ring_38be: bool,
    /// `20ae:38ba` / `.SAV 0x21e` -- кастет. The four weapon flags gate each
    /// other's damage bonuses in the post-kill item table
    /// (`1000:552c`..`1000:57cc`), so all four have to be carried even
    /// though none of them is read anywhere else.
    pub weapon_kastet_38ba: bool,
    /// `20ae:394b` / `.SAV 0x2af` -- дубинка (`1000:55a0` gate).
    pub weapon_dubinka_394b: bool,
    /// `20ae:38c2` / `.SAV 0x226` -- ножик (`1000:568e` gate).
    pub weapon_nozhik_38c2: bool,
    /// `20ae:394c` / `.SAV 0x2b0` -- тесак (`1000:5734` gate).
    pub weapon_tesak_394c: bool,
    /// `20ae:38b4` / `.SAV 0x218` -- костюм Abibas, `mar` row 4
    /// (`1000:bf80` sets it, `^1Костюм Abibas(+1) ` at `1000:22a1`).
    ///
    /// **Task 26 made all six writable in play.** They were carried but
    /// unreachable until then -- only a loaded `.SAV` could set one -- and
    /// [`Game::buy_market_row`] now sets each from its own arm
    /// (`1000:bf80`, `1000:c029`, `1000:c0e0`, `1000:c183`, `1000:c222`,
    /// `1000:c2ca`). [`Game::imm_row_visible`]'s `abs` term still ignores
    /// all four of the armour-bearing ones, which is the same divergence
    /// `docs/re/gaps.md` records; closing that is the gym's recompute
    /// (`1000:e3a4`..`1000:e3e2`), not this shop's, and it stays open.
    pub wear_suit_abibas_38b4: bool,
    /// `20ae:38b5` / `.SAV 0x219` -- Бутсы (`1000:c029`, `1000:1e81`).
    pub wear_boots_38b5: bool,
    /// `20ae:38b6` / `.SAV 0x21a` -- Кожанка, `mar` row 6 (`1000:c0e0`,
    /// `1000:2323`).
    pub wear_jacket_38b6: bool,
    /// `20ae:38b7` / `.SAV 0x21b` -- костюм Adidas, `mar` row 7
    /// (`1000:c183`, `1000:22fc`).
    pub wear_suit_adidas_38b7: bool,
    /// `20ae:38b8` / `.SAV 0x21c` -- Понтовые бутсы (`1000:c222`,
    /// `1000:1ecf`).
    pub wear_boots_pontovye_38b8: bool,
    /// `20ae:38b9` / `.SAV 0x21d` -- Крутая кожанка, `mar` row 9
    /// (`1000:c2ca`, `1000:237e`).
    pub wear_jacket_krutaya_38b9: bool,
    /// Where [`Game::mage_save`](crate::persist) and any other writer put
    /// their files.
    ///
    /// The original writes into the process's current directory -- every
    /// filename it builds is either bare (`save_r0.sav`, `places.sav`) or
    /// prefixed with `GetDir(0)` plus a backslash (`1000:6a2d`..`1000:6a55`),
    /// which is the same directory. `"."` is therefore the faithful default.
    /// It is a field rather than a `current_dir()` call so a test can point a
    /// save at a scratch directory instead of dropping `save_r0.sav` into the
    /// working tree, which is what `cargo test` would otherwise do the first
    /// time a test answers `y` to the mage.
    pub save_dir: std::path::PathBuf,
    /// `20ae:3951` / `.SAV 0x2b5` -- the church's sermon stage, 0..2. Read
    /// at `1000:7c76`/`1000:7ceb`/`1000:7dcb` to pick which sermon runs and
    /// at `1000:8247` to pick the parting line.
    pub church_visits: u8,
    mode: Mode,
    /// The fight recorder, `None` unless [`Game::start_fight_log`] asked for
    /// it. See that method for what it is for.
    fight_log: Option<FightLog>,
    /// The most recently fought opponent, shown by `Command::Inspect` (`sv`).
    last_enemy: Option<Fighter>,
    running: bool,
}

impl Game {
    /// Start a brand-new character.
    ///
    /// **Established from flow.** The original's new-character block is three
    /// consecutive stores at `1000:6dbe`:
    ///
    /// ```text
    /// 6dbe  c6 06 92 36 01   mov byte [0x3692],1   ; district := 1
    /// 6dc3  c6 06 98 36 01   mov byte [0x3698],1   ; Vet    discovered
    /// 6dc8  c6 06 94 36 01   mov byte [0x3694],1   ; Market discovered
    /// ```
    ///
    /// `0x3698` and `0x3694` are two of the seven contiguous discovery flags
    /// at `20ae:3694..369a` (see [`Game::enter_shop`]), so **a brand-new
    /// character already has the vet and the market**. An earlier revision of
    /// this comment cited only the load-failure string and stopped one
    /// instruction short of `6dc3`, which left both locations permanently
    /// unreachable in this port.
    ///
    /// Three paths reach `1000:6dbe`, and all three write all three bytes:
    ///
    /// * `1000:6b3a` -- `1000:6b33` `cmp byte [0x3d04],0` / `ja 0x6b3d`
    ///   falls through when the `save_r?.sav` scan (`1000:6a62` zeroes the
    ///   counter, `1000:6a8a` `FindFirst`, `1000:6ab9` `inc byte [0x3d04]`)
    ///   found no save file. **This is the path a fresh run with no `.SAV`
    ///   files in the working directory takes**, and it prints nothing.
    /// * `1000:6b81` -- the save-slot prompt `^0Нажми цифру с какого района
    ///   начать. 1-начать сначала` (file `0x7C69`, written at `1000:6b51`)
    ///   read a key into `[0x3d31]` that is none of `'0'`,`'2'`..`'5'`
    ///   (`1000:6b5e`..`1000:6b7f`) -- i.e. the player pressed `1`,
    ///   "начать сначала". This is the path when the shipped `SAVE_R?.SAV`
    ///   files are present.
    /// * `1000:6bdd` -- the slot file's `Reset` (`1000:6bcf`, record size
    ///   `0x2b6`) left `IOResult` non-zero (`1000:6bd4` calls it,
    ///   `1000:6bdb` `jz 0x6be0` is the success arm); jumps to `1000:6da5`,
    ///   which writes
    ///   `^6Чё-то глюкануло - нaверно нет такого сейва, Default:1`
    ///   (file `0x7D21`) and falls straight through into `1000:6dbe`.
    ///
    /// That string is therefore evidence for *this block existing*, not for
    /// which path a new game takes; `district := 1` and both flags are
    /// common to all three. This port models the first path.
    ///
    /// Note that the `places.sav` load path (`1000:6c5a`, see
    /// [`crate::locations::TRACKED`]) never reaches `1000:6dbe`: its own
    /// failure block at `1000:6d3b` *clears* the flags and leaves via
    /// `1000:6da0` `jmp 0x7262`.
    ///
    /// The three stores cost no `Random` draw, so wiring them does not
    /// perturb the RNG sequence.
    pub fn new(player: Fighter, progress: Progress, seed: u32) -> Game {
        // 1000:6dc3 then 1000:6dc8, in that order.
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
            flag_3693: false,
            dark_glasses: false,
            prison_tattoo: false,
            oneshot_gift_1: false,
            oneshot_gift_2: false,
            ring_gospodi_pomilui: false,
            pontovost_street: 0,
            buff_countdown: 0,
            market_ban_countdown: 0,
            club_ban_countdown: 0,
            den_errand_1_pending: false,
            den_errand_2_pending: false,
            fight_accepted_3b72: false,
            pistol: crate::combat_dispatch::Pistol::default(),
            rector_showdown: false,
            dealer_delivery_counter: 0,
            den_loan_credit: 0,
            church_visits: 0,
            fight_log: None,
            tooth_guard: false,
            charm_krestik_38bd: false,
            charm_ring_38be: false,
            weapon_kastet_38ba: false,
            weapon_dubinka_394b: false,
            weapon_nozhik_38c2: false,
            weapon_tesak_394c: false,
            wear_suit_abibas_38b4: false,
            wear_boots_38b5: false,
            wear_jacket_38b6: false,
            wear_suit_adidas_38b7: false,
            wear_boots_pontovye_38b8: false,
            wear_jacket_krutaya_38b9: false,
            save_dir: std::path::PathBuf::from("."),
            mode: Mode::Street,
            last_enemy: None,
            running: true,
        };
        g.apply_class_bonus();
        g
    }

    /// `1000:7347`..`1000:73e5`, all of it inside `FUN_1000_6a0d`: the
    /// district-5 rector-showdown arm, then the class bonus, then the den's
    /// opening loan credit.
    ///
    /// **Established from flow.** `docs/re/wander.md` ("What reaches
    /// `1000:73bb`") shows both exits of the character-setup procedure
    /// converge on `1000:7262`, and `1000:7369`'s `jnz 0x73bb` skips only the
    /// district-1 intro text, never anything after it -- so this whole
    /// function runs on **every** entry into the game, new character or
    /// loaded save. Re-derived here (`1000:7240`..`1000:7369` re-disassembled
    /// for Task 20's review fix; `1000:73bb`..`1000:73e5` from the original
    /// port):
    ///
    /// ```text
    /// 7262  a0 92 36        mov al,[0x3692]   ; DISTRICT, not class
    /// 7265  3c 01           cmp al,1 / jnz 0x729e   ; -> district-1 intro,
    ///       ... (district 2, 3, 4 arms, each `jnz` to the next `cmp` --
    ///       none of them touches `al`) ...
    /// 7347  3c 05           cmp al,5 / jnz 0x7369   ; DISTRICT 5
    /// 734b  WriteLn file 0x81F5   ; ^1Пора наконец отомстить ректору...
    /// 7364  c6 06 83 3c 01  mov byte [0x3c83],1   ; rector_showdown
    /// 7369  80 3e 92 36 01  cmp byte [0x3692],1    ; district == 1?
    /// 73bb  a1 9c 38        mov ax,[0x389c]        ; CLASS, [0x389c]
    /// 73be  3d 05 00        cmp ax,5          ; Гопник
    /// 73c3  c6 06 96 36 01  mov byte [0x3696],1   ; Den
    /// 73ca  3d 03 00        cmp ax,3          ; Подтсан
    /// 73cf  c6 06 97 36 01  mov byte [0x3697],1   ; Girl
    /// 73d4  c6 06 99 36 01  mov byte [0x3699],1   ; Club
    /// 73db  3d 06 00        cmp ax,6          ; Вор
    /// 73e0  c6 06 95 36 01  mov byte [0x3695],1   ; Dealers
    /// 73e5  c6 06 35 3e 05  mov byte [0x3e35],5   ; den loan credit
    /// ```
    ///
    /// **`1000:7347`..`1000:7364` reads `[0x3692]`, the DISTRICT, not
    /// `[0x389c]`, the class** -- a review fix for Task 20, which first
    /// shipped this arm mislabelled as "class-5 character creation" and, on
    /// that wrong reading, invented a "set twice for a Гопник" story that
    /// does not exist: nothing about class selects this arm at all. What it
    /// really is: whenever `FUN_1000_6a0d` runs with district already at 5
    /// -- which for a **loaded save** can be true on the very first entry,
    /// before a single turn is played -- it arms `rector_showdown` and
    /// prints the line, exactly once (the arm falls straight through to
    /// `1000:7369`, never looping). This is the "settling address" for the
    /// loaded-save divergence `docs/re/gaps.md`'s "The district-advance
    /// autosave — wired (Task 21)" records: not a main-loop rewrite,
    /// because `apply_class_bonus` is already the port's home for
    /// everything else `FUN_1000_6a0d` re-applies on load
    /// (`src/persist.rs`'s `from_save` calls it for exactly that reason),
    /// and `self.district` is available here the same way `self.player.class`
    /// already is.
    ///
    /// `1000:734b`'s line is a SEPARATE copy of the same text
    /// [`Game::enter_district_5`] prints from `1000:adc3` (different file
    /// offset, `0x81F5` vs `0x9CF2`, same 35 bytes) -- the original repeats
    /// itself, this port reproduces both sites rather than reusing one
    /// string constant for two different original addresses. The two never
    /// fire for the same game: this one only at entry when district is
    /// ALREADY 5 (so [`Game::district_advance`], gated on `district < 5` at
    /// `1000:ab88`, cannot also have fired for that game),
    /// and [`Game::enter_district_5`] only at the turn district first
    /// BECOMES 5 during play (so this arm, which only runs once at entry,
    /// already ran before that point and found district `< 5`).
    ///
    /// The three class arms are mutually exclusive (`1000:73c8` and
    /// `1000:73d9` jump straight to `1000:73e5`), and `1000:73e5` is
    /// unconditional. Class 4 (Отморозок) gets no flag here -- its bonus is
    /// the +1 HP per walk at `1000:b2d4`.
    pub(crate) fn apply_class_bonus(&mut self) {
        if self.district == 5 {
            term::println("^1Пора наконец отомстить ректору...");
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

    /// The banner is printed once by `main.rs` before character creation,
    /// matching a DOS splash-then-prompt startup; `run()` itself does not
    /// print it again (only `Command::Version` calls [`Game::banner`]).
    ///
    /// **The loop starts with [`Game::district_advance`], not with the
    /// prompt.** That is `1000:ab75` in the original, and it is upstream of
    /// the street prompt within one turn: `1000:ae3c` writes the bare `\`
    /// (file `0x9BF1`) and `1000:ae55`..`1000:ae63` is the top-level `ReadLn`
    /// into `DS:3972`, both after the whole `ab75`..`ae18` region. The back
    /// edge that closes the turn is `1000:ee01 e9 71 bd` `jmp 0xab75`, so
    /// the advance is the FIRST thing every turn does; `1000:ab72
    /// e8 98 be` `call 0x6a0d` is a three-byte near call whose next
    /// instruction is `1000:ab75` itself, so the very first pass is reached
    /// by fall-through out of character setup -- which is where `main.rs`
    /// hands control to this method.
    ///
    /// **Only `Mode::Street` turns pass through it**, established from flow:
    /// each shop handler writes its own prompt and `ReadLn`s into `DS:3a72`
    /// inside its own loop (`1000:bd08`/`1000:bd21` for `mar`,
    /// `docs/re/command-dispatch.md`, "Shop modality"), and never reaches
    /// `1000:ee01`. `Mode::Shop` is this port's line-at-a-time stand-in for
    /// that inner loop, so running the advance on those iterations would
    /// promote the player on turns the original does not.
    ///
    /// **That gate is established from flow and is currently unobservable,
    /// which is stated rather than covered by a test that could not fail.**
    /// [`Game::district_advance`] returns without reading or printing unless
    /// a promotion is due, so removing the gate would only differ while the
    /// player is inside a shop AND `level >= district * 10` AND
    /// `district < 5`. This port cannot reach that state, and the reason is
    /// that **the level only rises on STREET turns**. Exactly one function
    /// increments it -- `progress::apply_levels`, whose `f.level += 1` is
    /// the file's only such statement and is not under a `#[cfg(test)]`
    /// (`src/progress.rs` has none: `grep -c '#\[cfg(test)\]'
    /// src/progress.rs` prints `0`). `progress::demote` is the only other
    /// writer of the field in `src/progress.rs` and it decrements.
    /// `apply_levels` has three
    /// callers -- `grep -n 'progress::apply_levels(' src/game.rs | grep -v
    /// '///'` (the `grep -v` drops this very comment, which the plain
    /// command matches: a citation that quotes its own search string is a
    /// fourth hit) -- of which the last is below this file's own
    /// `#[cfg(test)]`, leaving two:
    ///
    /// * [`Game::run_combat`]'s post-fight award, entered from
    ///   [`Game::walk`], which is `Command::Walk` and so a Street turn;
    /// * [`Game::church`]'s zero arm (`1000:7f68`, selected by draw 15's
    ///   `Random(5)` at `1000:7f63`), which sets `xp := threshold`
    ///   (`1000:7fe4`/`1000:7fe7`) and forces a level. The church is reached
    ///   from the wander preamble's draw 13 -- `Random(200)` at
    ///   `1000:b39e`, calling `1000:b3a7` on a zero -- which is also inside
    ///   [`Game::walk`], hence also a Street turn.
    ///
    /// An earlier revision of this paragraph said "the level only rises in
    /// combat", which the church arm refutes; the conclusion is unchanged
    /// because both callers sit on a Street turn. So the advance that
    /// collects the new level runs at the top of the very next iteration,
    /// clearing all seven discovery flags on the way
    /// (`Places::reset_for_new_district`), and no shop is enterable
    /// afterwards either. It becomes reachable the moment the class-
    /// conditional spare at `1000:abc9` (class 5 keeps the Den) is
    /// implemented; `docs/re/gaps.md` records that as open.
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        while self.running {
            if matches!(self.mode, Mode::Street) {
                self.district_advance(&mut lines)?;
                if !self.running {
                    break;
                }
            }
            self.prompt();
            let Some(line) = lines.next() else { break };
            let line = line?;
            match self.mode.clone() {
                Mode::Street => {
                    let cmd = parse(&line);
                    self.dispatch(cmd, &mut lines)?;
                }
                Mode::Shop(loc) => self.shop_turn(loc, &line, &mut lines)?,
            }
        }
        Ok(())
    }

    /// `1000:ab75`..`1000:ad12` -- the district-advance preamble, and the
    /// autosave prompt hanging off it. Runs at the top of every street turn
    /// (see [`Game::run`] for why that placement is the original's).
    ///
    /// **Established from flow**, re-disassembled for this task with
    /// `python3 tools/re_query.py resolve 1000:ab75 -n 420 -i 200`:
    ///
    /// ```text
    /// ab75  a0 92 36 / 30 e4 / ba 0a 00 / f7 e2   ax := district * 10
    /// ab7f  3b 06 a6 38 / 7e 03   cmp ax,[0x38a6] / jle 0xab88   ; level
    /// ab85  e9 90 02              jmp 0xae18   -- gate 1 failed
    /// ab88  80 3e 92 36 05 / 72 03  cmp byte [0x3692],5 / jb 0xab92
    /// ab8f  e9 86 02              jmp 0xae18   -- gate 2 failed
    /// ab92  fe 06 92 36           inc [0x3692]
    /// ab96..abc9                  the discovery-flag resets
    /// abce  c6 06 76 3b 00        [0x3b76] := 0   ; market ban countdown
    /// abd3  c6 06 77 3b 00        [0x3b77] := 0   ; club ban countdown
    /// abec  WriteLn cs:0x82b3     ; file 0x9B83 = decimal 39811
    /// ac05  WriteLn cs:0x82fd     ; file 0x9BCD = decimal 39885
    /// ac1e  Write   cs:0x8321     ; file 0x9BF1, the bare `\` -- no newline
    /// ac31  ReadLn  -> DS:3a72    ; 0f78:06c6 / 0f78:059d / 0f78:0291
    /// ac45  call 0eed:0216        ; the case-fold
    /// ac54  0f78:0bd8 vs cs:0x8323 ; file 0x9BF3 = the single character `y`
    /// ac59  74 03 / e9 b4 00      jz 0xac5e, else jmp 0xad12
    /// ac73  Str([0x3692]) -> DS:3b7c, width 0
    /// ac88  DS:3d32 (the directory) -> tmp, then `save_r`, the digit, `.sav`
    /// acab  0f78:072e  Assign
    /// acb9  0f78:0772  Rewrite(f, 0x2b6)   ; 694
    /// acc8  0f78:0825  BlockWrite from DS:369c
    /// acd5  0f78:07ea  Close
    /// ad0d  WriteLn `^1Сохранено в save_r` (cs:0x8331, file 0x9C01 = 39937)
    ///       then the digit,
    ///       then the suffix `.sav` at cs:0x832c (file 0x9BFC = 39932)
    /// ```
    ///
    /// The two announcement lines, taken verbatim from `data/strings.json`'s
    /// `text` field rather than retyped:
    ///
    /// `^1Ты доказал, что ты самый крутой в этом районе - отправляйся в следующий`
    /// -- file 0x9B83, decimal 39811.
    /// `^0Хочешь сохранить свои достижения?`
    /// -- file 0x9BCD, decimal 39885.
    ///
    /// **The block cannot loop, so at most ONE district is gained per
    /// turn.** Every branch inside `ab75`..`ad12` is forward -- `ab83`,
    /// `ab85`, `ab8d`, `ab8f`, `aba5`, `abb6`, `abc7`, `ac59`, `ac5b` -- and
    /// the only branch instruction in the whole image whose target is
    /// `0xab75` is `1000:ee01`, at the very END of the turn. A raw byte scan
    /// for every `jmp`/`Jcc`/`call`/`loop` encoding of that target returns
    /// **two** hits; the other, `1000:ab00` `72 73`, is the `rs` of
    /// `^4Gopnik: ^7version 1.02 june,` inside the CS literal pool
    /// (`0x82b3`..`0xab59`, the gap between `FUN_1000_7c67` and `entry` in
    /// `data/functions.json`), and it passes the 64-way alignment sweep
    /// 63/64 -- the exact `1000:d83b` failure `docs/re/METHODOLOGY.md`
    /// warns about, reproduced here on a different address.
    ///
    /// **This is where the port used to diverge.** The promotion lived in
    /// [`Game::run_combat`]'s post-fight block as a `while` loop, which
    /// promoted a level-40 district-1 character four districts inside one
    /// fight; the original needs four turns, and awards the first of them on
    /// the turn AFTER the fight that raised the level -- the level moves
    /// inside `FUN_1000_3d11` (`1000:51ed`..`1000:5238`, reached from the
    /// wander at `1000:aea1`), which is downstream of `ab75` in the same
    /// turn. `1000:ab92` is the only in-play write to `[0x3692]`: the other
    /// three direct stores (`1000:6bf9`, `1000:6d9d`, `1000:6dbe`) are all
    /// inside `FUN_1000_6a0d`, the one-time setup, and the one remaining
    /// writer, `0f78:134c bf 92 36 mov di,0x3692` + `rep stosw`, is the
    /// runtime's startup BSS zero-fill of `20ae:3692`..`20ae:4118`
    /// (`python3 tools/re_query.py xrefs-to 20ae:3692` -- 97 accepted
    /// references, 0 discarded; `docs/re/gaps.md` carries the full decode.
    /// An earlier revision said "exactly four write" and missed the
    /// pointer-form fifth). `FUN_1000_3d11` contains no write -- but it does
    /// contain twelve `a0 92 36 mov al,[0x3692]` READS, so the claim is
    /// "no write", never "no reference"; a later revision widened it to the
    /// latter and `docs/re/gaps.md` lists all twelve.
    ///
    /// **Two port decisions, neither a property of the original.**
    ///
    /// * A failed write is reported and the turn continues, the same shape
    ///   [`Game::mage`] already uses and for the same reason: nothing between
    ///   `1000:acb9` and `1000:acd5` tests `IOResult`, so the original has no
    ///   failure message here at all, and swallowing a host I/O error
    ///   silently would be worse than one line the original never prints.
    /// * EOF on the prompt ends the run rather than being read as "not `y`".
    ///   The original blocks in `ReadLn`; a line-based port has no such
    ///   state, and every other `lines.next()` in this file treats `None`
    ///   the same way.
    ///
    /// **What is still NOT reproduced here**, and why it is not this
    /// method's job:
    ///
    /// * The discovery-flag resets are `Places::reset_for_new_district`,
    ///   which clears all seven unconditionally while `1000:aba0`,
    ///   `1000:abb1` and `1000:abc2` spare Club and Girl for class 3 and the
    ///   Den for class 5. That divergence predates this task and is recorded
    ///   in `src/locations.rs`'s module doc and `docs/re/gaps.md`.
    /// * `1000:ad12`'s district-keyed announcement arms (`cmp al,2` and the
    ///   chain after it) are unported text.
    /// * The chapter-5 arm's two forced fights, `1000:ae2d`
    ///   `FUN_1000_3d11(3)` and `1000:ae39` `FUN_1000_3d11(4)`. Those, and
    ///   only those: the arm's own prints and its `1000:addc` `ReadKey`
    ///   (`1000:adc3`..`1000:ae13`) run exactly ONCE in the original too,
    ///   because `1000:ab8d`'s `jb` fails at district 5 and `1000:ab8f`
    ///   jumps straight to `1000:ae18`, so `1000:ad12`..`1000:adbf` are
    ///   unreachable on a non-promotion turn. What repeats every turn is
    ///   `1000:ae18`'s arm -- the idempotent Den grant at `1000:ae1f` plus
    ///   those four calls. [`Game::enter_district_5`] reproduces everything
    ///   except the calls; their `param_1` handling is what
    ///   [`Game::run_combat`] does not model. An earlier revision of this
    ///   bullet said `1000:adbf`'s `cmp al,5` made the whole arm repeat;
    ///   that was wrong, and `docs/re/gaps.md` carries the branch-target
    ///   scans that settle it.
    ///
    /// `pub` for the same reason [`Game::walk`] and [`Game::mage`] are: it is
    /// the only way a test can reach this arm. [`Game::run`] reads from
    /// `io::stdin()` directly, so nothing in-process can drive the hook
    /// through its real call site.
    pub fn district_advance(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        // 1000:ab7f -- `district * 10 <= level`. The original's `jle` is the
        // signed form and `20ae:38a6` is a Pascal Integer; `player.level` is
        // never negative here, so the unsigned compare agrees.
        if u16::from(self.district) * 10 > self.player.level {
            return Ok(());
        }
        // 1000:ab88 -- `district < 5`, unsigned (`72` / `jb`).
        if self.district >= 5 {
            return Ok(());
        }
        self.district += 1; // 1000:ab92
        self.places.reset_for_new_district(); // 1000:ab96..1000:abc9
        self.market_ban_countdown = 0; // 1000:abce
        self.club_ban_countdown = 0; // 1000:abd3
        term::println("^1Ты доказал, что ты самый крутой в этом районе - отправляйся в следующий");
        term::println("^0Хочешь сохранить свои достижения?");
        // 1000:ac1e is `0eed:0000`, the no-newline Write -- the same call and
        // the same string (`cs:0x8321`) the street prompt at 1000:ae3c uses.
        term::print("\\");
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        // 1000:ac45's case-fold, then 1000:ac54's compare against `y`.
        //
        // `.trim()` is a PORT ADDITION and a real (if tiny) divergence:
        // 1000:ac45 case-folds the whole DS:3a72 buffer and 1000:ac54 hands
        // it straight to `0f78:0bd8` `rtl_str_compare`, which compares the
        // shortstring's length byte too -- so `" y"` is length 2 against
        // length 1 and the original refuses it, while this port saves. The
        // port's line source (`BufRead::lines`) has already stripped the
        // terminator the original's `ReadLn` also strips, so the trim only
        // affects genuine leading/trailing spaces. Kept for consistency with
        // the identical idiom in [`Game::mage`] and in
        // `crate::commands::parse` rather than introduced here; the whole
        // nine-site class is recorded in `docs/re/gaps.md`, "The trimmed `y`
        // prompts", instead of nine separate comments.
        if answer.trim().eq_ignore_ascii_case("y") {
            // 1000:ac5e..1000:ac73 `Str([0x3692])` -- the district AFTER the
            // increment above, which is why the shipped corpus is
            // `SAVE_R2`..`SAVE_R5` and has no `SAVE_R1`. Both gates bound it
            // to 2..=5, so it is always one digit.
            let digit = char::from(b'0' + self.district);
            let name = crate::persist::slot_filename(digit);
            let dir = self.save_dir.clone();
            // 1000:acb9/1000:acc8 write the 694-byte RECORD and nothing else:
            // the only `Rewrite` in `ab75`..`ad12` is at `acb9`, the only
            // `BlockWrite` at `acc8`, and `1000:acd5 Close` follows it
            // directly. The mage's `places.sav` pass (1000:766f..1000:7724)
            // has no counterpart here.
            match self.write_save_as(&dir, &name) {
                Ok(_) => term::println(&format!("^1Сохранено в save_r{digit}.sav")),
                Err(e) => term::println(&format!("^6{e}")),
            }
        }
        // 1000:adbf, reached from 1000:ad12's compare chain via
        // 1000:ad89's `jnz 0xadbf` -- NOT by fall-through, which
        // 1000:adbd `eb 59 jmp short 0xae18` blocks. It is taken whichever
        // way the `y` compare above went, because 1000:ac5b jumps into
        // 1000:ad12 as well. (An earlier revision of this comment said
        // "fall-through"; the doc on `enter_district_5` below and
        // `docs/re/gaps.md` both had it right, so this file contradicted
        // itself.)
        if self.district == 5 {
            self.enter_district_5(lines);
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
            Mode::Shop(Location::Dealers) => "^0Барыги\\",
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

    fn dispatch(
        &mut self,
        cmd: Command,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
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
            // file 0xB58A -- the inner "^6w^7" markup is part of the string.
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
            Command::Joint => self.smoke(Joint::Street),
            Command::Drink => self.beer(Beer::One),
            Command::BingeDrink => self.beer(Beer::Binge),
            Command::SellJunk => self.sell_junk(),
            Command::SellItems => self.sell_items(),
            // An unmatched line writes nothing at all: the last compare in
            // the chain (`exit`/`e`, 1000:edfa) falls through to
            // `jmp 0xab75` at 1000:ee01, straight back to the top of the
            // loop, with no output in between. The `^4? <input>` line an
            // earlier revision printed here was composed, not a real string.
            Command::Unknown(_) => {}
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
            Location::Dealers => {
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
    /// A scan of `orig/g.exe` for `c6 06 [94-9a] 36 imm8` finds 31 stores to
    /// these seven bytes: 14 clears and **17** set-to-1. **Twelve of the
    /// seventeen are implemented in this port and five are not**, which is
    /// the split `docs/re/gaps.md`'s 17-row inventory records. Of the twelve,
    /// the four listed next predate Task 11c; the other eight are named in
    /// the Task 11c paragraph below. All twelve are established from flow and
    /// all were re-derived from `orig/g.exe`. (An earlier revision of this
    /// comment said "Four of the seventeen … implemented here" and then "Five
    /// … remain unimplemented", which adds to nine, not seventeen: it counted
    /// only this list and forgot the eight it goes on to describe.)
    ///
    /// * `1000:6dc3` `c6 06 98 36 01` and `1000:6dc8` `c6 06 94 36 01` --
    ///   the **vet's** and the **market's** flags, written by the
    ///   new-character block at `1000:6dbe` ([`Game::new`]).
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
    /// So the market and the vet are open from turn one, and `w` -> girl ->
    /// club is a real, reachable chain.
    ///
    /// **All seven flags are reachable in this port** as of Task 11c, which
    /// implemented the other **eight** of the twelve: the wander preamble's
    /// four discovery rolls (`1000:b196`, `1000:b1c8`, `1000:b1fa`,
    /// `1000:b22c` -- [`Game::wander_preamble`]) and the four `[0x389c]`
    /// progression reveals (`1000:73c3`, `1000:73cf`, `1000:73d4`,
    /// `1000:73e0` -- [`Game::apply_class_bonus`]). Den comes
    /// from the class-5 bonus (`1000:73c3`), Dealers from the class-6 bonus
    /// (`1000:73e0`), and Gym from draw 8 (`1000:b21c` `Random(100)`, store
    /// at `1000:b22c`) — a 1-in-100 roll per walk, so *rare*, not
    /// unreachable. A revision of this comment written before Task 11c said
    /// "Dealers, Den and Gym stay unreachable"; that is now false on all
    /// three counts and contradicted `docs/re/gaps.md`'s own inventory.
    ///
    /// **All seventeen are now implemented, as of Task 20.** The de-level
    /// penalty (`1000:4aa5` -- [`Game::flee_penalty`]) and the post-kill
    /// block (`1000:52b3` -- [`Game::claim_spoils`]) were *already* ported
    /// before Task 20 -- an earlier revision of this comment (and of
    /// `docs/re/gaps.md`'s own inventory) claimed both were still missing,
    /// which was wrong; the code was checked against that claim and the
    /// code is what stands. Task 20 closed the two rows that genuinely were
    /// open: the `a` token's two stores (`1000:dcf6`/`1000:dcfb` --
    /// [`Game::den_reveal`]) and the chapter-5 endgame arm's flag store and
    /// Den grant (`1000:ae1f` -- [`Game::enter_district_5`], which does NOT
    /// port that arm's two forced fights -- see its own doc comment). 17 + 0
    /// = 17. The complete 17-row inventory, with a trigger and an evidence
    /// tier per row, is in `docs/re/gaps.md`, "Discovery flags: the
    /// complete store inventory".
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
    /// * `1000:d728` -- `Random(2)` (`1000:d724` is the `mov ax,2`,
    ///   `1000:d727` the `push`); on `0` (`1000:d72d` `or ax,ax` /
    ///   `1000:d72f` `jnz 0xd756`), and only if the club is still
    ///   undiscovered (`1000:d731` `cmp byte [0x3699],0`), prints file
    ///   `0xB48C` (`1000:d738` `mov di,0x9bbc`) and sets
    ///   the club's discovery flag at `1000:d751`. This is one of the two
    ///   discovery paths in the game.
    /// * `1000:d756`/`1000:d76f` -- files `0xB4CD`, `0xB4F6`.
    /// * `1000:d788`..`1000:d793` -- `hp := hpmax`, `money -= 12`, and
    ///   `1000:d793` `c6 06 76 3b 00` clears the market ban countdown
    ///   `20ae:3b76`. **Not modelled here**, and the field it would clear
    ///   *does* exist ([`Game::market_ban_countdown`]) -- nothing in this
    ///   port ever sets it non-zero, so clearing it would be a no-op. The
    ///   whole omission -- both setters, both gates and this clear -- is
    ///   registered in `docs/re/gaps.md`, "The two ban countdowns are
    ///   modelled and decremented but never set". An earlier revision of this
    ///   line said "(not modelled here)" before the field existed and was
    ///   never revisited.
    fn visit_girl(&mut self) {
        if self.player.money < 12 {
            term::println("^6Ну непойдёшь же как придурок без ничего.");
            return;
        }
        term::println("^2Ты пришел к своей подруге.");
        if self.rng.below_at("1000:d728", 2) == 0 && !self.places.is_found(Location::Club) {
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
    /// `rep`, `kl` and `trn` price their rows with an instruction immediate
    /// rather than a byte out of the `20ae:0b2e` array, so they are not in
    /// `data/shops.json`; Task 12 traced all nine of them and they are in
    /// [`IMM_ROWS`]. See `docs/re/difftest.md`.
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
                self.print_imm_rows("rep");
            }
            Location::Den => {
                // 1000:d816..1000:d8b9 then 1000:d8b9..1000:dae2 -- one
                // straight-line run with no branch between them; the split
                // into two methods is the port's, not the original's.
                self.print_den_intro();
                self.print_den_menu();
            }
            Location::Club => {
                term::println("Ты пришел в клуб напиши  ^6w^7  чтобы уйти");
                term::println(" Здесь можно сыграть в карты (^6p^7 Минимальная ставка- 5р.)");
                self.print_imm_rows("kl");
            }
            Location::Gym => {
                term::println("Ты пришел в качалку напиши  ^6w^7  чтобы уйти");
                self.print_imm_rows("trn");
            }
            Location::Girl | Location::Street | Location::Temple | Location::Dorm => {}
        }
    }

    /// The nine "^6N^7 - ^" prefixes (N = the row's digit), file `0xA4A2`
    /// upward.
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

    /// Which of `tag`'s rows the menu LISTS, in image order -- the district
    /// filter and nothing else.
    ///
    /// Split out so the filter has exactly one implementation. It is the
    /// **menu** half of the district's two uses: at `bmar` these five gates
    /// (`1000:c68d`, `1000:c6f1`, `1000:c755`, `1000:c7ba`, `1000:c81d`) are
    /// the only ones there are, and [`Game::shop_action`]'s buy path
    /// deliberately does not consult them. A test that re-implemented this
    /// predicate instead of calling it would pass with the gate deleted --
    /// round 1 of this task shipped exactly that -- so
    /// `a_gated_dealers_row_is_bought_below_its_district` calls this.
    fn listed_rows(&self, tag: &str) -> Vec<&'static data::ShopEntry> {
        data::shops()
            .iter()
            .filter(|r| r.shop == tag && self.gate_open(r.gate))
            .collect()
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

    /// What a priced row's `#` placeholders are filled with, in the order the
    /// original pushes them.
    ///
    /// ## Why surplus values are ignored, and where the surplus is
    ///
    /// **Established from flow, and this is the premise both fills below rest
    /// on.** Every priced menu line is written by one `call 0eed:01c2`
    /// (`System.WriteLn`), and **all 24 of those call sites in the two menu
    /// blocks push exactly five words**. Measured over an aligned linear walk
    /// of `entry` from `1000:ab59` (7742 instructions, its whole recorded
    /// `size`), filtered to `1000:b94a`..`1000:bd08` for `mar` and
    /// `1000:c4be`..`1000:c8ce` for `bmar` -- each block from its shop-tag
    /// compare to its prompt push, the same `shop_tag_at` anchors
    /// `data/shop_arms.json` records, and **all four bounds are instruction
    /// boundaries the walk reaches**. 24 sites, and the push-count histogram
    /// is `{5: 24}` -- not one site with four or six. The slots a row does
    /// not use are `xor ax,ax` / `push ax`, so a row with one `#` pushes its
    /// price and then **four zeros**, and Borland's `Write` consumes the
    /// placeholders left to right and drops what is left over. That is why
    /// `text::fill` taking more values than the template has `#`s is the
    /// original's own behaviour and not a port convenience
    /// (`crate::text::tests::fill_drops_surplus_values` asserts it).
    ///
    /// **The lower bound is load-bearing.** Starting the `mar` filter at
    /// `1000:b930` -- not an instruction boundary; a cold decode there reads
    /// `rcl [bx+si+0x31],0xc0` -- sweeps in a 25th site at `1000:b93b`, which
    /// is the `WriteLn` immediately *before* the `mar` verb compare at
    /// `1000:b94a` and belongs to the handler that precedes the market, not
    /// to its menu. Its five words are all zeros, so it changes no
    /// conclusion, but it is a row that is not a row: hence the aligned
    /// anchor.
    ///
    /// The 120 pushed words split exactly **97 zeros + 18 price bytes + 5
    /// immediates**. The 18 is the eighteen priced rows, one price byte each
    /// (`mov al,[imm8]` / `xor ah,ah`), which is the cross-check that the
    /// sweep found every row and no extra site. The five immediates belong to
    /// three rows:
    ///
    /// | row | surplus pushes | consumed? |
    /// |---|---|---|
    /// | `mar` 2 | `1000:ba5a mov ax,0x5` | yes -- the second `#` |
    /// | `bmar` 5 | `1000:c6df mov ax,0x5` | **no -- discarded** |
    /// | `bmar` 6 | `1000:c743 mov ax,0x5` | **no -- discarded** |
    /// | `bmar` 7 | `1000:c7a7 mov ax,0x14`, `1000:c7ab mov ax,0x1e` | yes -- the second and third `#` |
    ///
    /// **`1000:c6df` and `1000:c743` are surplus the original itself throws
    /// away, and no row is broken by it.** Their strings hold one `#` each
    /// and bake their numbers into the text --
    /// CS `0x92b8` (pushed at `1000:c6cf`) is `#^7 руб. Кастет(урон+2)` and
    /// CS `0x92d0` (pushed at `1000:c733`) is
    /// `#^7 руб. Дубинка(урон+4), заменяет кастет` -- so the `5` each pushes
    /// lands in no slot. Both are recorded here so a later task opening
    /// `bmar`'s remaining arms reads them as *discarded*, not as a
    /// placeholder this helper forgot; neither address had appeared anywhere
    /// in `src/` or `docs/` before this note.
    ///
    /// ## The two rows that DO consume a surplus push
    ///
    /// Sixteen of the eighteen rows in `data/shops.json` hold exactly one
    /// `#` and it is the price. **Two hold more, and every extra one is an
    /// instruction immediate rather than a price.** Both were printing a
    /// bare `#` on screen until Task 26. The inventory was first taken from
    /// the `data/shops.json` side alone -- counting `#` in the template --
    /// and the complementary sweep above, over which call sites push a
    /// non-price immediate, is what found the other two rows and made
    /// "sixteen" a measured number rather than a stopped search.
    ///
    /// **`mar` row 2, `#^7 руб.  Пиво(#з)`.** The line is assembled at
    /// `1000:ba4a mov di,0x8bfe`; then `1000:ba54 mov al,[0xb2f]` /
    /// `1000:ba57 xor ah,ah` / `1000:ba59 push ax` pushes the price byte
    /// `20ae:0b2f`, and `1000:ba5a mov ax,0x5` / `1000:ba5d push ax` pushes
    /// a literal `5` straight after it. `1000:ba5a` is file `0xD32A`, the
    /// address `docs/re/tables.md` §2 records for it. So the screen reads
    /// `Пиво(5з)` -- and it would read `Пиво(5з)` even if the price byte held
    /// something else, because the two 5s only coincide. Filling the second
    /// `#` from `displayed_price` would be that coincidence dressed as a
    /// rule, which is why the literal is written out.
    ///
    /// **`bmar` row 7, `... ^6f^7 урон(#-#).`** Three placeholders, not two:
    /// `1000:c7a1 mov al,[0xb3e]` pushes the price, then
    /// `1000:c7a7 mov ax,0x14` and `1000:c7ab mov ax,0x1e` push 20 and 30.
    /// **The 30 is the original's own off-by-one, reproduced**: the shot the
    /// row is advertising rolls `20 + Random(10)` -- `1000:4f14 mov ax,0xa`
    /// and `1000:4f1d add ax,0x14`, ported in [`crate::combat_dispatch::fire`]
    /// -- so the real range is 20..=29 and the menu says 20-30. The line is
    /// printed from its own immediates, not from the shot's, so the port
    /// prints 30 here and rolls 29 there, as the original does.
    ///
    /// Both extras are keyed by shop and row rather than derived, because
    /// nothing in `data/shops.json` carries them: the artifact records the
    /// price array `20ae:0b2e`.. and these are not in it.
    fn row_fill_values(row: &data::ShopEntry) -> Vec<i64> {
        let mut values = vec![row.displayed_price as i64];
        match (row.shop, row.key) {
            ("mar", "2") => values.push(5),           // 1000:ba5a, file 0xD32A
            ("bmar", "7") => values.extend([20, 30]), // 1000:c7a7, 1000:c7ab
            _ => {}
        }
        values
    }

    /// The [`IMM_ROWS`] belonging to `tag`, in image order, each gated by
    /// [`Game::imm_row_visible`].
    ///
    /// Assembly is the same three parts as [`Game::print_priced_rows`]:
    /// prefix, affordability colour digit, row text. The one `#` in the
    /// whole table -- `trn` row 3's `10^7  прокачать # качков опыта` -- is
    /// filled from a *separate* immediate, `1000:e505` `mov ax,0xa`, which
    /// happens to equal that row's price at `1000:e4c4`; the fill below uses
    /// `price` and both immediates are checked against the image by
    /// `tools/difftest.py`.
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
    /// The vet's two rows have no gate of their own (the whole menu is
    /// skipped when the player is unhurt, at `1000:d3d3`). The other seven
    /// are gated, each by a test that sits immediately before the row's own
    /// `cmp word [20ae:38c7],imm8`:
    ///
    /// | row | gate | address |
    /// |---|---|---|
    /// | `kl` 1 | none | -- |
    /// | `kl` 2 | `district > 1` | `1000:dfc4` `cmp byte [0x3692],1` / `jbe 0xe020` |
    /// | `trn` 1 | none | -- |
    /// | `trn` 2 | none | -- |
    /// | `trn` 3 | `district > 1` **and** `district * 10 - 3 > level` | `1000:e4aa`; `1000:e4b1`..`1000:e4c2` (`mul 10`, `sub ax,3`, `cmp ax,[0x38a6]`, `jle 0xe51a`) |
    /// | `trn` 4 | `district > 1` | `1000:e51a` |
    /// | `trn` 5 | `district > 2` **and** `abs < district * 2` | `1000:e576`; `1000:e57d`..`1000:e58d` (`shl ax,1`, `mov al,[0x3e34]`, `cmp ax,dx`, `jge 0xe5e4`) |
    ///
    /// `abs` is the scratch byte `20ae:3e34`, recomputed on every entry to
    /// the gym at `1000:e3a4`..`1000:e3e2`: it starts as the armour byte
    /// `20ae:38b2` and then has the armour that came from *equipment*
    /// subtracted back out --
    ///
    /// * `1000:e3aa`..`1000:e3b8`: `-1` when `[0x38b4]` is set and
    ///   `[0x38b7]` is not,
    /// * `1000:e3bc`..`1000:e3c3`: `-2` when `[0x38b7]` is set,
    /// * `1000:e3c8`..`1000:e3d6`: `-2` when `[0x38b6]` is set and
    ///   `[0x38b9]` is not,
    /// * `1000:e3db`..`1000:e3e2`: `-4` when `[0x38b9]` is set,
    ///
    /// so it is the part of the armour the player trained rather than
    /// bought. Those four bytes are the ownership flags for four `mar` rows:
    /// `1000:bf80` sets `[0x38b4]` (row 4, the abibas suit, "Смягчает пинок
    /// на 1"), `1000:c183` sets `[0x38b7]` (row 7, adidas, "на 2"),
    /// `1000:c0e0` sets `[0x38b6]` (row 6, the leather jacket, "защиты ... на
    /// 2") and `1000:c2ca` sets `[0x38b9]` (row 9, "Броня +4") -- the four
    /// subtrahends are those four rows' own advertised bonuses.
    ///
    /// **The port carries all four flags and this method still ignores
    /// them**, so `abs` is exactly `armor` here, where the original computes
    /// `armor` minus 1 for `[38b4]` without `[38b7]`, minus 2 for `[38b7]`,
    /// minus 2 for `[38b6]` without `[38b9]`, and minus 4 for `[38b9]`.
    /// Only one row reads `abs`: `("trn","5")`. It has **two** gates, and
    /// only the second reads `abs` -- `1000:e576` `cmp byte [0x3692],0x2` /
    /// `jbe` is `district > 2`, and `1000:e57d`..`1000:e58d` is
    /// `abs < district * 2`. This method implements both; the prose used to
    /// fold them into one. So the whole consequence is that this port can
    /// HIDE a gym row the original shows.
    ///
    /// **This became live in Task 19 and is not fixed here.** Before it,
    /// nothing in the port could set the four bytes (buying a `mar` row
    /// deducts the price and prints the text but applies no effect), so the
    /// divergence was theoretical. A loaded `.SAV` sets them, and it has a
    /// concrete witness in the shipped corpus: **`SAVE_R4` at slot 4** holds
    /// `38b4`/`38b6`/`38b7` set and `38b9` clear with `armour` 10, so the
    /// original computes `abs = 10 - 2 - 2 = 6` against a threshold of 8 and
    /// shows the row, while this port computes `abs = 10` and hides it.
    /// (`SAVE_R3` and `SAVE_R5` agree either way; no shipped save carries
    /// all four flags -- an earlier revision of this comment said `SAVE_R3`
    /// and `SAVE_R4` did, and the frozen corpus refutes it: both carry
    /// three.)
    ///
    /// Correcting it means deciding what a `mar` purchase does, which is the
    /// unimplemented shop-effects gap and a different task's subject;
    /// applying the subtraction here while purchases still grant nothing
    /// would make the gym row depend on a flag the player cannot earn.
    /// Registered in `docs/re/gaps.md`, "The four armour flags are carried
    /// but the gym's `abs` ignores them".
    fn imm_row_visible(&self, row: &ImmRow) -> bool {
        let district = i32::from(self.district);
        let level = i32::from(self.player.level);
        let abs = i32::from(self.player.armor);
        match (row.shop, row.key) {
            ("kl", "2") => district > 1,
            ("trn", "3") => district > 1 && district * 10 - 3 > level,
            ("trn", "4") => district > 1,
            ("trn", "5") => district > 2 && abs < district * 2,
            _ => true,
        }
    }

    /// `pr`, `1000:d816`..`1000:d8b9`. `Ты пришел в притон - ` (file
    /// `0xB5C0`) is *written without a newline* (`call 0eed:0000` at
    /// `1000:d82a`), then exactly one district-keyed suffix completes the
    /// line. District 1 spends a real `Random(6)` draw for the dorm number
    /// (`1000:d83f`, `+3`), so this branch is part of the RNG sequence.
    ///
    /// The call site is `1000:d83f`, not `1000:d83b`: re-derived from an
    /// aligned start at `1000:d816`, `1000:d83b` is `b8 06 00`
    /// (`mov ax,0x6`), `1000:d83e` is `50` (`push ax`) and `1000:d83f` is
    /// the `9a 4b 11 78 0f` (`call 0f78:114b`), with `1000:d844`
    /// `05 03 00` (`add ax,0x3`) after it. A four-byte-early label costs
    /// nothing while the branch never fires, but `site` is the identity key
    /// of the differential replay in `tests/wander_sequence.rs`, so a future
    /// capture that drove the den would report a spurious mismatch.
    ///
    /// The conditional lines that follow (`1000:d8b9` onward) are
    /// [`Game::print_den_menu`], ported by Task 28; this method is menu
    /// lines 0..4 of `data/den_arms.json`'s seventeen and nothing else.
    fn print_den_intro(&mut self) {
        term::print("Ты пришел в притон - ");
        match self.district {
            1 => {
                let n = self.rng.below_at("1000:d83f", 6) + 3;
                term::println(&text::fill("^0общагу №#", &[n as i64]));
            }
            2 => term::println("^0общагу ВКИ"),
            3 => term::println("^0гоповский притон"),
            4 => term::println("^0притон отморозков"),
            // No `else`: `1000:d82f`, `1000:d859`, `1000:d879` and
            // `1000:d899` are four independent `cmp byte [0x3692],N`
            // blocks, the last of which falls through to `1000:d8b9`. A
            // district outside 1..=4 -- reachable, since
            // [`Game::district_advance`] promotes while `district < 5`
            // (`1000:ab88`) and so can leave it at 5 -- writes the
            // prefix and nothing more: no suffix, and no newline either,
            // because the prefix went out through `0eed:0000` (`Write`)
            // and no `WriteLn` follows.
            _ => {}
        }
    }

    /// The den's menu, `1000:d8b9`..`1000:dae2` -- lines 5..16 of
    /// `data/den_arms.json`'s seventeen, the twelve
    /// [`Game::print_den_intro`] does not print. `docs/re/den.md`, "The
    /// menu", is the map; every gate, string and colour store below cites
    /// its own address.
    ///
    /// **They print ONCE, on entry.** Established from flow in
    /// `docs/re/den.md`: every branch in the image whose target is the
    /// prompt push `1000:dae2` is one of exactly three -- `1000:dac0` and
    /// `1000:dac7` (line 16's own gate misses, i.e. the entry from the
    /// menu) and `1000:dede` (the `w` compare's miss, the loop's only back
    /// edge). So the back edge lands on the PROMPT, never on the menu, and
    /// this method belongs in [`Game::print_shop_intro`] and not in
    /// [`Game::shop_turn`].
    ///
    /// | # | gate | address |
    /// |---|---|---|
    /// | 5 | -- | a bare `WriteLn` on `20ae:3fcc`, `1000:d8be` -- one blank line, no literal |
    /// | 6 | `1000:d8c8 cmp byte [0x3b78],0x1` / `jnz 0xd8e8` | errand one pending |
    /// | 7 | `1000:d8e8 cmp byte [0x3b79],0x0` / `jz 0xd90f` **and** `1000:d8ef cmp word [0x38cb],0x64` / `jl 0xd90f` | errand two AND cred >= 100 |
    /// | 8 | threshold block #1, `1000:d90f`..`1000:d941` | [`Game::den_menu_reveal_hint`] |
    /// | 9 | -- | a second blank `WriteLn`, `1000:d961` |
    /// | 10 | -- | unconditional |
    /// | 11 | colour only, `1000:d984 cmp word [0x38c3],0x0` / `jnz 0xd992` | the `p` row |
    /// | 12 | `1000:d9ec cmp byte [0x3e35],0x0` / `jbe 0xda35`, colour `1000:d9d9 cmp word [0x38cb],0x2` / `jnl 0xd9e7` | the `r` row |
    /// | 13 | `1000:da35 cmp byte [0x3b78],0x1` / `jnz 0xda55` | the `hp` row |
    /// | 14 | -- | unconditional |
    /// | 15 | threshold block #2, `1000:da6e`..`1000:daa0` | the `a` row |
    /// | 16 | `1000:dabb cmp word [0x38cb],0x64` / `jl 0xdae2` **and** `1000:dac2 cmp byte [0x3b79],0x0` / `jz 0xdae2` | the `d` row |
    ///
    /// **The two dimmed rows.** Rows 11 and 12 are built with the same
    /// three-call idiom `docs/re/tables.md` records for the priced shop
    /// rows, writing the colour digit to `20ae:3b7a` first: `0x34` (ASCII
    /// `4`, dim) at `1000:d98b` / `1000:d9e0` and `0x30` (ASCII `0`) at
    /// `1000:d992` / `1000:d9e7`, then `1000:d9ad` / `1000:da09`
    /// `mov al,[0x3b7a]` loads it back between the prefix `Напиши ^`
    /// (CS `0x9dcb`) and the row text. This port computes the digit inline
    /// rather than carrying `20ae:3b7a` as a field.
    ///
    /// **That is not a shortcut around row 12's ordering.** `1000:d9d9`'s
    /// colour store runs BEFORE `1000:d9ec`'s visibility gate, so the
    /// original writes `20ae:3b7a` even on a turn where the `r` row is not
    /// printed. It is unobservable: `20ae:3b7a` has 87 image-wide
    /// references and every one of them is the same store-store-load
    /// triple (`data/den_arms.json`'s `globals[]` record for it), so no
    /// reader anywhere reaches the byte without its own writer running
    /// first, and row 12's is the last store in the den either way.
    ///
    /// **Nothing here is a `Random` site.** The whole `1000:d8b9`..
    /// `1000:dae2` span holds no `call 0f78:114b`: `data/den_arms.json`'s
    /// draw sweep over the range returns exactly five sites and they are
    /// `1000:d83f` (the intro, ported), `1000:dd97`, `1000:ddda`,
    /// `1000:de5a` and `1000:de7c` (all four in the `d` arm).
    fn print_den_menu(&self) {
        // 1000:d8be `call 0f78:05dd` / 1000:d8c3 `call 0f78:0291` -- a bare
        // Pascal `WriteLn` on the output `Text` at 20ae:3fcc, i.e. a blank
        // line with no literal pushed.
        term::println("");
        // 1000:d8c8, string CS 0x9d46 pushed at 1000:d8cf.
        if self.den_errand_1_pending {
            term::println("^6На одного пацана наехал какой-то урод");
        }
        // 1000:d8e8 and 1000:d8ef -- a conjunction: either miss lands on the
        // same 1000:d90f. `jl` is signed. String CS 0x9d6e at 1000:d8f6.
        if self.den_errand_2_pending && self.pontovost_street >= 0x64 {
            term::println("^6Ты пацан нормальный. Есть дело.");
        }
        // 1000:d90f..1000:d941, threshold block #1. String CS 0x9d90 at
        // 1000:d943, printed by 1000:d957.
        if self.den_menu_reveal_hint() {
            term::println("^6Пацаны хотят тебе кое-чё сказать");
        }
        // 1000:d961 -- the second bare `WriteLn`.
        term::println("");
        // 1000:d96b, string CS 0x9db3, printed by 1000:d97f.
        term::println("Напиши ^6w^7 чтобы уйти");
        // Row 11: prefix CS 0x9dcb (1000:d99d) + the colour digit + suffix
        // CS 0x9dd4 (1000:d9bb), one `WriteLn` at 1000:d9d4. The line ALWAYS
        // prints; only its colour depends on the beer count.
        term::println(&format!(
            "Напиши ^{}p^7  чтобы угостить пацанов пивом",
            // 1000:d984 `cmp word [0x38c3],0x0` / 1000:d989 `jnz 0xd992`:
            // a NON-ZERO beer count takes the `jnz` to the '0' store. The
            // test is equality, not order, so this is `!= 0` and not `> 0`
            // -- unlike the `p` arm's own `jle` gate at 1000:db38.
            if self.player.beer_dl != 0 { "0" } else { "4" }
        ));
        // Row 12: same prefix CS 0x9dcb (1000:d9f9) + digit + suffix
        // CS 0x9df6 (1000:da17), one `WriteLn` at 1000:da30.
        //
        // 1000:d9ec `cmp byte [0x3e35],0x0` / `jbe 0xda35` is UNSIGNED on a
        // byte compared with zero, so it refuses exactly `== 0`.
        if self.den_loan_credit != 0 {
            term::println(&format!(
                "Напиши ^{}r^7  чтобы занять 2 рубля",
                // 1000:d9d9 `cmp word [0x38cb],0x2` / 1000:d9de `jnl 0xd9e7`
                // -- signed, `>= 2` is the normal colour.
                if self.pontovost_street >= 2 { "0" } else { "4" }
            ));
        }
        // 1000:da35, string CS 0x9e10 at 1000:da3c, printed by 1000:da50.
        if self.den_errand_1_pending {
            term::println("Напиши ^6hp^7 чтобы отпинать мудака который наезжал на пацана");
        }
        // 1000:da55, string CS 0x9e4e, printed by 1000:da69. Unconditional.
        term::println("Напиши ^6s^7  чтобы узнать отношение");
        // 1000:da6e..1000:daa0, threshold block #2 -- BYTE-IDENTICAL to
        // block #1 and NOT the block the `a` arm uses. String CS 0x9e73 at
        // 1000:daa2, printed by 1000:dab6.
        if self.den_menu_reveal_hint() {
            term::println("Напиши ^6a^7  чтобы спросить чё-то");
        }
        // 1000:dabb (signed `jl`) and 1000:dac2 -- both misses land on the
        // prompt push 1000:dae2. String CS 0x9e96 at 1000:dac9, printed by
        // 1000:dadd.
        if self.pontovost_street >= 0x64 && self.den_errand_2_pending {
            term::println("Напиши ^6d^7 чтобы пойти на дело");
        }
    }

    /// Threshold blocks **#1** (`1000:d90f`..`1000:d941`, menu line 8) and
    /// **#2** (`1000:da6e`..`1000:daa0`, menu line 15, the `a` row).
    ///
    /// **These two are one predicate and the `a` ARM's is a different one.**
    /// Established from flow by re-slicing the bytes out of `orig/g.exe`
    /// (`data/den_arms.json`'s `threshold_blocks[]` carries all three byte
    /// strings): blocks #1 and #2 are **52 bytes each and byte-identical**,
    /// branch displacements included, while block #3 -- `1000:dcba`, the one
    /// [`Game::den_reveal`] implements -- is **43 bytes**. The nine missing
    /// bytes are exactly `1000:d92f sub ax,0x5` (`2d 05 00`),
    /// `1000:d932 mov si,ax` (`8b f0`), the *second* `1000:d936 shl ax,1`
    /// (`d1 e0`) and `1000:d938 add ax,si` (`01 f0`). So with
    /// `k = level - (district-1)*10`, this predicate is `5k - 25 + cred >= 40`
    /// and the arm's is `2k + cred >= 40`.
    ///
    /// **Neither implies the other**, which is why they are two methods and
    /// not one: `k = 1, cred = 38` satisfies the arm and not this, so the
    /// reveal fires with no menu line offering it; `k = 13, cred = 0`
    /// satisfies this and not the arm, so the menu offers `a` and the arm
    /// refuses in silence.
    /// `the_den_menu_hint_and_the_a_arm_disagree_in_both_directions` drives
    /// both of those states.
    ///
    /// The shared thirteen-byte prefix IS identical in all three:
    /// `1000:d90f cmp byte [0x3695],0x0` / `1000:d914 jz 0xd91d` /
    /// `1000:d916 cmp byte [0x369a],0x0` / `1000:d91b jnz 0xd95c`, so the
    /// skip happens only when Dealers **and** Gym are both already found --
    /// Dealers clear takes the `jz` straight past the Gym test.
    ///
    /// The arithmetic runs in 16-bit `ax` in the original and `1000:d93e
    /// cmp ax,0x28` / `1000:d941 jl 0xd95c` is a SIGNED compare; this port
    /// widens to `i32`, exactly as [`Game::den_reveal`] already does, so a
    /// `[0x38cb]` large enough to wrap `ax` would diverge. Unreachable at
    /// any value the port can produce.
    fn den_menu_reveal_hint(&self) -> bool {
        if self.places.is_found(Location::Dealers) && self.places.is_found(Location::Gym) {
            return false;
        }
        // 1000:d91d..1000:d93a: ax := level - (district-1)*10, minus 5,
        // times 5 (`mov si,ax` / `shl ax,1` / `shl ax,1` / `add ax,si`),
        // plus [0x38cb].
        let level_in_district = i32::from(self.player.level) - (i32::from(self.district) - 1) * 10;
        (level_in_district - 5) * 5 + self.pontovost_street >= 0x28
    }

    /// One turn at a location's own prompt. The location's keys are checked
    /// *before* the street verb table, because the original reads them with
    /// its own `ReadLn DS:3a72` that never reaches `entry`'s `DS:3972`
    /// dispatch chain at all -- this is why the vet's `h` (heal a jaw) and
    /// the street's `h` (drink a beer, `1000:e966`) can share a letter.
    /// `w`/`run` leaves, which every location's intro text names as the way
    /// out. Anything else is ignored and the prompt repeats.
    ///
    /// ## The den's seven keys
    ///
    /// Each is established at its own `0f78:0bd8` compare against the den's
    /// own buffer `20ae:3a72`, never from a menu string (`docs/re/den.md`,
    /// "The arms"): `p` `1000:db2c`, `r` `1000:db81`, `hp` `1000:dc04`,
    /// `s` `1000:dc6d`, `a` `1000:dcef`, `d` `1000:dd3c`, `w` `1000:ded7`.
    ///
    /// `hp` is the one arm whose gate stands **in front of** its own key
    /// compare rather than behind it: `1000:dbf3 cmp byte [0x3b78],0x1` /
    /// `1000:dbf8 jnz 0xdc63`, so with no errand pending the token is never
    /// compared and the line falls straight through to the `s` compare.
    /// `a` is the same shape with threshold block #3 in front of it
    /// ([`Game::den_reveal`]).
    ///
    /// **An unrecognised key prints nothing.** `1000:dede jmp 0xdae2` is
    /// the loop's only back edge from below and it targets the PROMPT, not
    /// the menu; there is no "unknown command" literal anywhere in
    /// `1000:d802`..`1000:df06` for it to print (`data/den_arms.json`'s
    /// string sweep over the range: 45 pushes, none of them a refusal for a
    /// bad key). The `_ => {}` fall-through below is that.
    ///
    /// **The den's `ReadLn` does not trim.** `1000:db1d call 0eed:0216`
    /// only lowercases ASCII `A`..`Z` -- it compares against no `0x20` --
    /// so ` p` is a miss in the original and a hit here. That is the
    /// existing trimmed-prompt divergence in `docs/re/gaps.md`, which the
    /// den now joins; the `.trim()` below is deliberately left alone rather
    /// than special-cased for one location.
    fn shop_turn(
        &mut self,
        loc: Location,
        line: &str,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        let key = line.trim().to_lowercase();
        match (loc, key.as_str()) {
            (Location::Vet, "h") => self.heal_jaw(),
            (Location::Vet, "r") => self.heal_leg(),
            (Location::Dealers, "x") => self.sell_junk(),
            (Location::Dealers, "wes") => self.sell_items(),
            // 1000:db2c, key literal CS 0x9ec1.
            (Location::Den, "p") => self.den_beer(),
            // 1000:db81, key literal CS 0x9a50.
            (Location::Den, "r") => self.den_borrow(),
            // 1000:dbf3 gates 1000:dc04, key literal CS 0x9f82. The guard is
            // the arm's own, not a dispatch condition the port invented: a
            // `hp` typed with no errand pending reaches 1000:dc63 and is
            // compared against `s`, which it is not.
            (Location::Den, "hp") if self.den_errand_1_pending => {
                return self.den_beat_up(lines);
            }
            // 1000:dc6d, key literal CS 0x9f85.
            (Location::Den, "s") => self.den_regard(),
            // 1000:dcef behind threshold block #3 at 1000:dcba.
            (Location::Den, "a") => self.den_reveal(),
            // 1000:dd3c, key literal CS 0xa036.
            (Location::Den, "d") => return self.den_job(lines),
            (Location::Market | Location::Dealers, k)
                if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) =>
            {
                self.shop_action(k.chars().next().unwrap());
            }
            _ => {
                // The den's own exit compare is 1000:ded7 against CS 0x848e
                // (`w`), whose hit jumps out at 1000:dee1. That literal is
                // shared by nine push sites image-wide, so it is every
                // location's exit key rather than the den's own, which is
                // why this stays one shared arm.
                if matches!(parse(line), Command::Walk) {
                    self.location = Location::Street;
                    self.mode = Mode::Street;
                }
                // Everything else: ignored, prompt repeats.
            }
        }
        Ok(())
    }

    /// `a` at the den prompt -- `1000:dcba`..`1000:dd32`, the hidden
    /// Dealers+Gym reveal. `docs/re/wander.md`, "The `a` reveal's input
    /// buffer" already established the token is read at the den's own
    /// `ReadLn DS:3a72`, not the street prompt; this is that arm.
    ///
    /// **Established from flow**, re-disassembled for this task
    /// (`python3 tools/re_query.py resolve 1000:dcba -n 200 -i 60`):
    ///
    /// ```text
    /// dcba  cmp byte [0x3695],0 / jz 0xdcc8    ; Dealers CLEAR -> compute
    /// dcc1  cmp byte [0x369a],0 / jnz 0xdd32   ; (else) Gym SET -> skip
    /// dcc8  ax := (level - (district-1)*10) * 2 + pontovost_street
    /// dce0  cmp ax,0x28 / jl 0xdd32            ; need >= 40
    /// dce5  push 0x3a72 (the buffer) / push 0x9fc9 ("a", file 0xB899)
    /// dcef  call 0f78:0bd8 / jnz 0xdd32        ; string compare
    /// dcf6  mov byte [0x3695],1                ; Dealers
    /// dcfb  mov byte [0x369a],1                ; Gym
    /// dd00  WriteLn file 0xB89B
    /// dd19  WriteLn file 0xB8CE
    /// dd32  (next token in the chain)
    /// ```
    ///
    /// So the skip at `dcc6` happens only when **both** Dealers and Gym are
    /// already set: Dealers clear takes the `jz` straight past the Gym test
    /// (Gym's own state never gates it), and Dealers set + Gym clear falls
    /// through to `dcc8` exactly like Dealers clear does -- the arm still
    /// runs. Getting `74`/`75` backwards here would flip which of "both
    /// set" and "Dealers set, Gym clear" is the skip.
    ///
    /// **The reveal prints two lines**, established by reading past the two
    /// stores rather than inferred: `dd00`..`dd19` and `dd19`..`dd2d` are
    /// each a `mov di,<string>` / `push cs` / `push di` / five zeroed
    /// `WriteLn` format-spec words / `call 0eed:01c2` (`docs/re/rtl.md:476`
    /// itself lists `0eed:01c2` unnamed; `docs/re/character-sheet.md:191`
    /// and `docs/re/branches.md:359` are what name it `WriteLn`) -- the
    /// same shape every other plain string print in this module uses --
    /// and execution falls straight
    /// through from the first into the second with no branch between them,
    /// landing on `dd32` right after, which is also both early-out targets'
    /// destination.
    ///
    /// **The two stores are unconditional once reached**: `dcf6` and `dcfb`
    /// are back-to-back five-byte immediate stores with no compare and no
    /// branch between or before them (after the `jnz 0xdd32` at `dcf4`), so
    /// Dealers and Gym are set even when one of them was already set --
    /// there is no compare instruction available to gate it on.
    fn den_reveal(&mut self) {
        if self.places.is_found(Location::Dealers) && self.places.is_found(Location::Gym) {
            return;
        }
        let level_in_district = i32::from(self.player.level) - (i32::from(self.district) - 1) * 10;
        if level_in_district * 2 + self.pontovost_street < 0x28 {
            return;
        }
        // dcf6/dcfb store before dd00/dd19 print; matched here even though
        // nothing reads either flag in between, so there is no observable
        // difference -- this is a port, not just a functional match.
        self.places.mark_found(Location::Dealers);
        self.places.mark_found(Location::Gym);
        term::println("^0Тут у нас есть пара мест куда тебе стоит сходить");
        term::println("^2Ты узнал где находится качалка и где находятся барыги");
    }

    /// `p` at the den prompt -- `1000:db22`..`1000:db77`, treat the lads to
    /// beer. **Established from flow**, re-derived for this task with
    /// `python3 tools/re_query.py resolve 1000:db22 -n 40 -i 60`:
    ///
    /// ```text
    /// db22  bf 72 3a           mov di,0x3a72        ; the den's own buffer
    /// db27  bf c1 9e           mov di,0x9ec1        ; the key literal `p`
    /// db2c  9a d8 0b 78 0f     call 0f78:0bd8       ; the token compare
    /// db31  75 44              jnz 0xdb77           ; miss -> the `r` arm
    /// db33  83 3e c3 38 00     cmp word [0x38c3],0x0
    /// db38  7e 24              jle 0xdb5e           ; SIGNED: 0 and below refuse
    /// db3a  ff 0e c3 38        dec [0x38c3]         ; пиво -1
    /// db3e  83 06 cb 38 05     add word [0x38cb],0x5 ; понтовость +5
    /// db43  bf c3 9e           mov di,0x9ec3        ; the confirmation
    /// db57  call 0eed:01c2
    /// db5c  eb 19              jmp short 0xdb77
    /// db5e  bf fb 9e           mov di,0x9efb        ; the refusal
    /// ```
    ///
    /// One gate and two effects, in that order: the stores at `db3a`/`db3e`
    /// both run before the `db57` print. The confirmation string says the
    /// rise is 5 and `db3e`'s immediate is 5 -- checked against the bytes,
    /// not assumed from the wording. Nothing one-shot is consumed, so the
    /// arm is repeatable, and it spends no `Random` draw
    /// (`data/den_arms.json`'s draw sweep puts all four of the handler's
    /// unported draws in the `d` arm).
    ///
    /// `1000:db38` is `jle`, a SIGNED compare against zero, so a negative
    /// count would refuse too. `beer_dl` is a `u16` here, matching the
    /// original's `word`; the cast reproduces the signedness rather than
    /// silently reading it as `== 0`.
    fn den_beer(&mut self) {
        if (self.player.beer_dl as i16) <= 0 {
            // 1000:db5e, string CS 0x9efb, printed by 1000:db72.
            term::println("^6А нет у тебя пива.");
            return;
        }
        // 1000:db3a then 1000:db3e -- both stores run before the print.
        self.player.beer_dl -= 1;
        self.pontovost_street += 5;
        // 1000:db43, string CS 0x9ec3, printed by 1000:db57.
        term::println("^2Ты угостил пацанов пивом. Понтовость улутшилась на 5.");
    }

    /// `r` at the den prompt -- `1000:db77`..`1000:dbf3`, borrow two
    /// roubles. **Established from flow** (`docs/re/den.md`, "`r` --
    /// `1000:db77`"):
    ///
    /// ```text
    /// db77  mov di,0x3a72 / db7c mov di,0x9a50 / db81 call 0f78:0bd8
    /// db86  jnz 0xdbf3                       ; miss -> the `hp` arm
    /// db88  cmp byte [0x3e35],0x0
    /// db8d  jbe 0xdbda                       ; UNSIGNED byte vs 0: refuses `== 0`
    /// db8f  cmp word [0x38cb],0x0
    /// db94  jle 0xdbbf                       ; SIGNED
    /// db96  add word [0x38c7],0x2            ; money +2
    /// db9b  sub word [0x38cb],0x2            ; понтовость -2
    /// dba0  dec [0x3e35]                     ; the loan credit -1
    /// dba4  mov di,0x9f10  (printed at dbb8) ; the confirmation
    /// dbbf  mov di,0x9f49  (printed at dbd3) ; refusal: no понтовость
    /// dbda  mov di,0x9f66  (printed at dbee) ; refusal: credit exhausted
    /// ```
    ///
    /// **Two gates with two distinct, non-interchangeable refusals, and the
    /// order is the original's.** The credit is checked FIRST (`db8d`) and
    /// prints `^6Ты уже всю мелочь выгреб!`; the понтовость check
    /// (`db94`) prints `^6Ты не можешь занять денег.`. Swapping them would
    /// print the wrong line for a player who is out of both.
    ///
    /// `20ae:3e35` is the one-shot resource: it starts at 5 (`1000:73e5`)
    /// and is topped up once per walk while below `district * 10`
    /// (`1000:af19`), both of which [`Game::den_loan_credit`] already
    /// models -- so this arm is reachable in play, not only from a test.
    fn den_borrow(&mut self) {
        if self.den_loan_credit == 0 {
            // 1000:dbda, string CS 0x9f66, printed by 1000:dbee.
            term::println("^6Ты уже всю мелочь выгреб!");
            return;
        }
        if self.pontovost_street <= 0 {
            // 1000:dbbf, string CS 0x9f49, printed by 1000:dbd3.
            term::println("^6Ты не можешь занять денег.");
            return;
        }
        // 1000:db96, 1000:db9b, 1000:dba0 -- in that order.
        self.player.money += 2;
        self.pontovost_street -= 2;
        self.den_loan_credit -= 1;
        // 1000:dba4, string CS 0x9f10, printed by 1000:dbb8.
        term::println("^2Ты занял 2 рубля на пиво. Понтовость уменьшилась на 2.");
    }

    /// `hp` at the den prompt -- `1000:dbf3`..`1000:dc63`, beat up the lout
    /// who leaned on one of the lads. **Established from flow**
    /// (`docs/re/den.md`, "`hp` -- `1000:dbf3`"):
    ///
    /// ```text
    /// dbf3  cmp byte [0x3b78],0x1 / dbf8 jnz 0xdc63   ; the gate, AHEAD of the key
    /// dbfa  mov di,0x3a72 / dbff mov di,0x9f82 / dc04 call 0f78:0bd8
    /// dc09  jnz 0xdc63
    /// dc0b  mov al,0x1 / dc0e call 0x10d14            ; FUN_1000_0d14(1)
    /// dc11  mov byte [0x3b72],0x1                     ; the fight-accepted flag
    /// dc1c  mov di,0x90c0                             ; `^6Это `
    /// dc26  mov di,[0x3952] / dc2a mov cl,0x8 / dc2c shl di,cl / dc2e add di,0x2e
    /// dc39  mov di,0x90c7                             ; ` # уровня.`
    /// dc43  push [0x395c]                             ; the rolled level
    /// dc53  call 0eed:01c2
    /// dc58  mov al,0x6 / dc5b call 0x13d11            ; FUN_1000_3d11(6)
    /// dc5e  mov byte [0x3b78],0x0                     ; the errand is consumed
    /// ```
    ///
    /// The two near calls wrap: `dc0e`'s `rel16` sums to image `0x10d14`,
    /// which is `1000:0d14` modulo 64 KiB, and `dc5b`'s to `1000:3d11`.
    /// `dc26`..`dc2e` is `[0x3952] * 0x100 + 0x2e`, exactly the `ranks`
    /// table `data/string_tables.json` records (base file `0x123de` =
    /// `20ae:002e`, stride 256), which is what [`Game::rank_name`] indexes.
    ///
    /// **`roll_enemy(1)` is not a guess about the argument.** `1000:dc0b`
    /// pushes 1, and [`Game::roll_enemy`] already models `param_1` at both
    /// of its clamp sites (`1000:0da7`, `1000:0dba`): 1 clamps the class to
    /// 7, so this errand can never roll the class-8 `Мент`.
    ///
    /// ## `FUN_1000_3d11`'s `param_1 = 6` is NOT modelled, and it costs draws
    ///
    /// [`Game::run_combat`] takes no `param_1` and implements the
    /// `param_1 = 0` path the wander's `1000:b826` uses. Two sweeps over an
    /// aligned decode of the fight function's 6971 bytes (3043
    /// instructions, `data/den_arms.json`'s `fight_param_finding`) find
    /// every `[bp+0x4]` reference (exactly eight -- `1000:3d24`,
    /// `1000:5085`, `1000:5139`, `1000:51a6`, `1000:51ac`, `1000:51f6`,
    /// `1000:51fc`, `1000:57ce`) and every `cmp al,imm8` (exactly five, the
    /// dispatch chain `1000:3d24 mov al,[bp+0x4]` feeds: `1000:3d27` tests
    /// 0, `1000:3d2b` 6, `1000:3e8d` 1, `1000:3ead` 3, `1000:3f2b` 4). 0
    /// and 6 take the **same** target `1000:3d32`, and of the seven
    /// non-load `[bp+0x4]` tests only `1000:57ce` names 6.
    ///
    /// `1000:57ce`'s `1000:57d2 jnz 0x5838` makes the block exclusive to 6
    /// exactly `1000:57d4`..`1000:5838`, **47 instructions, decoded in
    /// full**, and it holds FIVE effects rather than the one an earlier
    /// revision of this comment claimed:
    ///
    /// * `1000:57d4`..`1000:57de` -- `add [0x38cb],ax`, `ax = district*20`;
    /// * `1000:57e2` / `1000:57fe` -- CS `0x3c99`, `#` = `district*20`;
    /// * `1000:5803` / `1000:581f` -- CS `0x3ce9`, `#` = `district*10`;
    /// * `1000:5824`..`1000:582e` -- `add [0x38ce],ax`, an **xp** award;
    /// * `1000:5832` / `1000:5835` -- `FUN_1000_2526(0)`, the capped
    ///   level-up drain, **which spends `Random` draws** at `1000:25fe`.
    ///
    /// The missing draws are the serious half: the RNG sequence is
    /// observable state, so a fight entered here leaves this port's
    /// generator at a different point from the original's. The extent is
    /// measured, not assumed -- over the 3043-instruction walk the block
    /// holds zero conditional branches, zero `jmp`s and zero
    /// `call 0f78:114b`, and no branch in the function targets any address
    /// inside it.
    ///
    /// **Where this stopped:** *when* the block runs is NOT established.
    /// `1000:57ce` has thirteen predecessors -- twelve branches plus the
    /// fall-through from `1000:57c9` -- and which fight outcomes reach them
    /// was not decoded. `docs/re/gaps.md`, "`FUN_1000_3d11`'s `param_1` --
    /// the den's two call sites", lists all thirteen and is the authority
    /// here; this port invents no condition and runs the fight through the
    /// unparameterised `run_combat`.
    fn den_beat_up(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        // 1000:dc0b/1000:dc0e -- FUN_1000_0d14(1).
        let enemy = self.roll_enemy(1);
        // 1000:dc11.
        self.fight_accepted_3b72 = true;
        // 1000:dc1c (CS 0x90c0), 1000:dc26..1000:dc2e (the rank name),
        // 1000:dc39 (CS 0x90c7) and 1000:dc43 (20ae:395c, the rolled
        // level) -- one `WriteLn` at 1000:dc53.
        term::print("^6Это ");
        term::print(&Self::rank_name(enemy.class));
        term::println(&text::fill(" # уровня.", &[enemy.level as i64]));
        // 1000:dc58/1000:dc5b -- FUN_1000_3d11(6). See the doc above.
        self.run_combat(enemy, lines)?;
        // 1000:dc5e, AFTER the fight returns.
        self.den_errand_1_pending = false;
        Ok(())
    }

    /// `s` at the den prompt -- `1000:dc63`..`1000:dcba`, ask how the lads
    /// regard you. **Established from flow** (`docs/re/den.md`, "`s` --
    /// `1000:dc63`"):
    ///
    /// ```text
    /// dc63  mov di,0x3a72 / dc68 mov di,0x9f85 / dc6d call 0f78:0bd8
    /// dc72  jnz 0xdcba
    /// dc74  mov di,0x9f87            ; `^4Твоя понтовость сейчас = #.`
    /// dc79  push [0x38cb]            ; the `#`
    /// dc89  call 0eed:01c2
    /// dc8e  mov al,[0x3692] / dc91 xor ah,ah / dc93 mov dx,0xa / dc96 mul dx
    /// dc98  add ax,0xa               ; district*10 + 10
    /// dc9b  cmp ax,[0x38cb] / dc9f jnle 0xdcba
    /// dca1  mov di,0x9fa5            ; the second line, printed at dcb5
    /// ```
    ///
    /// **This arm writes nothing**, and that is a measurement rather than
    /// an omission: `data/den_arms.json` records an absolute-write sweep
    /// over `1000:dc63`..`1000:dcba` that finds zero stores.
    /// `1000:dc79 push [0x38cb]` is a READ -- the `#` argument.
    ///
    /// The threshold arithmetic does **not** `dec ax` first, unlike all
    /// three `[0x3695]`/`[0x369a]` blocks: it is `district*10 + 10`, not
    /// `(district-1)*10`. `1000:dc9f jnle` skips the second line when
    /// `district*10 + 10 > [0x38cb]`, so the line prints on `<=`.
    fn den_regard(&self) {
        // 1000:dc74 (CS 0x9f87) + 1000:dc79, printed by 1000:dc89.
        term::println(&text::fill(
            "^4Твоя понтовость сейчас = #.",
            &[i64::from(self.pontovost_street)],
        ));
        // 1000:dc8e..1000:dc9f.
        if i32::from(self.district) * 10 + 10 <= self.pontovost_street {
            // 1000:dca1, CS 0x9fa5, printed by 1000:dcb5.
            term::println("^0Да если чё мы за тебя впрягаемся.");
        }
    }

    /// The `d` arm's luck roll -- `1000:dda6`..`1000:ddb3` and
    /// `1000:dde9`..`1000:ddf1`, ONE predicate evaluated twice with the
    /// branches permuted: `Longint([0x38a4]) < Longint(Random(district*15))`.
    ///
    /// **Established from flow.** `20ae:38a4` is Удача, named twice over
    /// and neither time from an adjacent string
    /// (`data/den_arms.json`'s `globals[]` record): it is the fourth and
    /// last of four stat words pushed into one `WriteLn` at `1000:1baa`,
    /// `1000:1bae`, `1000:1bb2`, `1000:1bb6`, whose format string is
    /// assembled from `Сл:^` / `#^7 Лв:^` / `#^7 Жв:^` / `#^7 Уд:^`
    /// (CS `0x16b7`, `0x16bc`, `0x16c5`, `0x16ce`), so the fourth argument
    /// is the one `Уд` labels; and `1000:4a50 dec [0x38a4]` is followed
    /// immediately, in the same basic block with no branch between, by
    /// `1000:4a54 mov di,0x3466` pushing `^4Удача -1 `.
    ///
    /// **The `JL` beside the `JB` is not a slip**, and reproducing it as a
    /// single signed or single unsigned compare would be a divergence. It
    /// is Borland's canonical 32-bit compare -- high halves SIGNED, low
    /// halves UNSIGNED -- and the 32-bit width comes from promoting
    /// `Random`'s `Word` result against the `Integer` at `20ae:38a4`:
    ///
    /// ```text
    /// dd9c  xor dx,dx     ; the random ZERO-extends into bx:cx
    /// dd9e  mov cx,ax
    /// dda0  mov bx,dx
    /// dda2  mov ax,[0x38a4]
    /// dda5  cwd           ; luck SIGN-extends into dx:ax
    /// dda6  cmp dx,bx
    /// dda8  jl 0xddb6     ; luck_hi <  random_hi (signed)   -> true
    /// ddaa  jle 0xddaf    ; luck_hi == random_hi            -> compare lows
    /// ddac  jmp 0xde36    ; luck_hi >  random_hi            -> false
    /// ddaf  cmp ax,cx
    /// ddb1  jb 0xddb6     ; luck_lo <  random_lo (UNSIGNED) -> true
    /// ddb3  jmp 0xde36
    /// ```
    ///
    /// The second copy is the same predicate with three branches permuted:
    /// `1000:ddeb jl 0xddf3` (true), `1000:dded jnle 0xde1a` (false),
    /// `1000:ddef cmp ax,cx` / `1000:ddf1 jnb 0xde1a` (false on `>=`, so
    /// true by fall-through on `<`). Every one of the ten branch
    /// instructions above is in `data/den_arms.json`'s `luck_compares[]`.
    ///
    /// `docs/re/wander.md`'s already-ported `1000:b5f1`..`1000:b61b` is the
    /// same idiom, and [`Game::walk`]'s own comment records that it widens
    /// both sides by zero-extension instead. That divergence is `walk`'s
    /// and is left where it is; this method does not inherit it.
    fn luck_below_random_32(luck: u16, random: u16) -> bool {
        // `cwd` on 1000:dda5 / 1000:dde8 vs `xor dx,dx` on 1000:dd9c /
        // 1000:dddf: only the LUCK side can be negative.
        let luck_high: i16 = if (luck as i16) < 0 { -1 } else { 0 };
        let random_high: i16 = 0;
        if luck_high != random_high {
            return luck_high < random_high; // 1000:dda8 / 1000:ddac, signed
        }
        luck < random // 1000:ddb1 / 1000:ddf1, unsigned
    }

    /// `d` at the den prompt -- `1000:dd32`..`1000:decd`, go on the job.
    /// The largest arm, and the only one with wide compares or draws
    /// (`docs/re/den.md`, "`d` -- `1000:dd32`").
    ///
    /// **Established from flow.**
    ///
    /// ```text
    /// dd32  mov di,0x3a72 / dd37 mov di,0xa036 / dd3c call 0f78:0bd8
    /// dd41  jz 0xdd46 / dd43 jmp 0xdecd
    /// dd46  cmp word [0x38cb],0x64 / dd4b jnl 0xdd50 / dd4d jmp 0xdecd  ; SILENT
    /// dd50  cmp byte [0x3b79],0x0  / dd55 jnz 0xdd5a / dd57 jmp 0xdecd  ; SILENT
    /// dd5a  mov di,0xa038  (printed at dd6e)   ; `^0Давай быстрее..`
    /// dd73  mov di,0xa04a  (printed at dd87)   ; `^2Ты пришел воровать деньги`
    /// dd8c..dd97   Random(district*15)         ; draw 1
    /// dda6..ddb3   luck < it ? 0xddb6 : 0xde36
    /// ddb6  mov di,0xa066  (printed at ddca)   ; `^4Шухер менты!`
    /// ddcf..ddda   Random(district*15)         ; draw 2
    /// dde9..ddf1   luck < it ? 0xddf3 : 0xde1a
    /// ddf3  mov al,0x2 / ddf6 call 0x10d14     ; FUN_1000_0d14(2) -- forces class 8
    /// ddf9  mov al,0x5 / ddfc call 0x13d11     ; FUN_1000_3d11(5)
    /// ddff  mov di,0xa075  (printed at de13)   ; `^6Пора валить!`
    /// de1a  mov di,0xa084  (printed at de2e)   ; `^2Ты смылся от ментов.`
    /// de36  mov di,0xa09b  (printed at de4a)   ; `^2Ты наваровал денег`
    /// de4f..de5a   Random(district*10)         ; draw 3
    /// de6d  add [0x38c7],ax                    ; money += district*10 + it
    /// de71..de7c   Random(district*10)         ; draw 4
    /// de8f  add [0x38c9],ax                    ; хлам, the same shape
    /// de93  mov di,0x908b / de98..dea2 district*12 (printed at deaf)
    /// deb4..debe  add [0x38ce],ax              ; xp += district*12
    /// dec2  mov al,0x0 / dec5 call 0x12526     ; FUN_1000_2526(0), the CAPPED form
    /// dec8  mov byte [0x3b79],0x0              ; errand two consumed
    /// ```
    ///
    /// **Both gates are silent on failure** -- `dd4d` and `dd57` jump
    /// straight to the `w` compare with nothing printed. Neither is a
    /// refusal string this port may invent: the string sweep over the whole
    /// handler finds none.
    ///
    /// **`dec8` runs on EVERY path that got past `dd55`**, both cop
    /// outcomes included: `de18 jmp short 0xde33` and `de33 jmp 0xdec8`
    /// carry the two cop arms there and the haul falls through into it.
    ///
    /// **Draw count per invocation, established from flow:** 3 on the haul
    /// path (`dd97`, `de5a`, `de7c`) and 2 in range on either cop path
    /// (`dd97`, `ddda`), plus whatever `1000:0d14` and `1000:3d11` spend.
    /// Reproduced exactly here -- the `n` of each draw was re-derived with
    /// `python3 tools/re_query.py pushed-n`, not copied from the fence.
    ///
    /// `dec5`'s `param_1 = 0` is the capped form (`1000:257a`), the same
    /// one the ordinary combat path passes at `1000:5238`, which is
    /// [`crate::progress::apply_levels`]'s `uncapped: false`. The award is
    /// the `add` at `debe`, not an argument -- so `apply_levels` gets it as
    /// `award` and does `p.xp += award` before the same drain.
    ///
    /// **`ddfc`'s `param_1 = 5` is NOT modelled.** Unlike `hp`'s 6, which
    /// takes the same `1000:3d32` target as 0, 5 matches none of the five
    /// values `FUN_1000_3d11` tests and reaches `1000:3fa7` directly, so it
    /// SKIPS the prologue 0 and 6 run.
    ///
    /// An earlier revision of this comment called that prologue
    /// `1000:3d32`..`1000:3fa7`. **That is an address interval, not an
    /// arm**: 310 instructions containing the rest of the dispatch chain
    /// (`1000:3e8d`, `1000:3ead`, `1000:3f2b`) and the entries of the arms
    /// for 1, 3 and 4 (`1000:3e91`, `1000:3eb1`, `1000:3f2f`). The arm
    /// itself is `1000:3d32`..`1000:3e8d` -- **168 instructions**, entered
    /// only by `1000:3d29 jz 0x3d32` and `1000:3d2d jz 0x3d32`, whose only
    /// exit is `1000:3e8a jmp 0x3fa7`, and `1000:3e8d` is reached by
    /// exactly one branch image-wide (`1000:3d2f jmp 0x3e8d`, the chain's
    /// own miss) so it is the next chain link and not part of the arm.
    ///
    /// **It spends no `Random` draw** -- zero `call 0f78:114b` across those
    /// 168 instructions -- so unlike the `hp` arm's residue this one cannot
    /// desynchronise the draw stream. **Where this stopped:** what the 168
    /// instructions DO was not decoded, so nothing here says what the cop
    /// fight gains or loses by skipping them. This port runs it through the
    /// unparameterised [`Game::run_combat`]; `docs/re/gaps.md`,
    /// "`FUN_1000_3d11`'s `param_1` -- the den's two call sites", is the
    /// authority.
    fn den_job(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        // 1000:dd46 / 1000:dd4b -- signed, and silent.
        if self.pontovost_street < 0x64 {
            return Ok(());
        }
        // 1000:dd50 / 1000:dd55 -- silent.
        if !self.den_errand_2_pending {
            return Ok(());
        }
        // 1000:dd5a (CS 0xa038, printed at 1000:dd6e) and 1000:dd73
        // (CS 0xa04a, printed at 1000:dd87).
        term::println("^0Давай быстрее..");
        term::println("^2Ты пришел воровать деньги");
        // 1000:dd8c..1000:dd94 build the `n`: [0x3692] * 15.
        let n15 = u16::from(self.district) * 15;
        let roll = self.rng.below_at("1000:dd97", n15);
        if Self::luck_below_random_32(self.player.luck, roll) {
            // 1000:ddb6, CS 0xa066, printed at 1000:ddca.
            term::println("^4Шухер менты!");
            // 1000:ddcf..1000:ddd7 rebuild the SAME `n` from scratch.
            let roll2 = self
                .rng
                .below_at("1000:ddda", u16::from(self.district) * 15);
            if Self::luck_below_random_32(self.player.luck, roll2) {
                // 1000:ddf3/1000:ddf6 -- param_1 = 2 forces class 8, the
                // `Мент` of data/string_tables.json's `ranks`.
                let cop = self.roll_enemy(2);
                // 1000:ddf9/1000:ddfc -- param_1 = 5, see the doc above.
                self.run_combat(cop, lines)?;
                term::println("^6Пора валить!"); // 1000:ddff, CS 0xa075
            } else {
                term::println("^2Ты смылся от ментов."); // 1000:de1a, CS 0xa084
            }
        } else {
            // 1000:de36, CS 0xa09b, printed at 1000:de4a.
            term::println("^2Ты наваровал денег");
            // 1000:de4f..1000:de6d and 1000:de71..1000:de8f: each of money
            // and хлам gains district*10 + Random(district*10), the base
            // recomputed from [0x3692] for every one of the four terms.
            let base = u16::from(self.district) * 10;
            let cash = i32::from(base) + i32::from(self.rng.below_at("1000:de5a", base));
            self.player.money += cash;
            let junk = base.wrapping_add(self.rng.below_at("1000:de7c", base));
            self.player.junk = self.player.junk.wrapping_add(junk);
            // 1000:de93 (CS 0x908b) + 1000:de98..1000:dea2, printed by
            // 1000:deaf -- BEFORE 1000:debe credits the same amount.
            let xp = u32::from(self.district) * 12;
            term::println(&text::fill(
                "^6Ты получаешь # качков опыта",
                &[i64::from(xp)],
            ));
            // 1000:deb4..1000:debe then 1000:dec2/1000:dec5.
            progress::apply_levels(
                &mut self.progress,
                &mut self.player,
                &mut self.rng,
                xp,
                false,
            );
        }
        // 1000:dec8 -- every path past 1000:dd55 reaches it.
        self.den_errand_2_pending = false;
        Ok(())
    }

    /// Everything [`crate::character_sheet::lines`] needs that is not a
    /// field of [`Fighter`], gathered off `self`.
    ///
    /// This is the port's stand-in for the original's argument convention,
    /// which is *no arguments at all*: `FUN_1000_1a03` ends in a bare `ret`
    /// at `1000:248e` and no instruction in its 2700 bytes uses a positive
    /// `bp` displacement, so it reads the player's DGROUP globals directly
    /// (`docs/re/character-sheet.md`, "The entry, and the argument
    /// convention"). Those globals are fields of `Game` here, so they are
    /// copied into the struct instead of being read out of a data segment.
    fn sheet_kit(&self) -> character_sheet::Kit {
        character_sheet::Kit {
            xp_38ce: self.progress.xp,
            threshold_38d0: self.progress.threshold,
            buff_countdown_38cd: self.buff_countdown,
            krestik_38bd: self.charm_krestik_38bd,
            ring_gs_38be: self.charm_ring_38be,
            ring_pg_38bf: self.oneshot_gift_1,
            mega_ring_38c0: self.oneshot_gift_2,
            ring_gp_38c1: self.ring_gospodi_pomilui,
            mobile_38bb: self.has_mobile,
            dark_glasses_38b3: self.dark_glasses,
            prison_tattoo_38bc: self.prison_tattoo,
            pistol: self.pistol,
            boots_38b5: self.wear_boots_38b5,
            boots_pontovye_38b8: self.wear_boots_pontovye_38b8,
            kastet_38ba: self.weapon_kastet_38ba,
            dubinka_394b: self.weapon_dubinka_394b,
            nozh_38c2: self.weapon_nozhik_38c2,
            tesak_394c: self.weapon_tesak_394c,
            tooth_guard_394a: self.tooth_guard,
            suit_abibas_38b4: self.wear_suit_abibas_38b4,
            suit_adidas_38b7: self.wear_suit_adidas_38b7,
            jacket_38b6: self.wear_jacket_38b6,
            jacket_krutaya_38b9: self.wear_jacket_krutaya_38b9,
        }
    }

    /// `s` -- `FUN_1000_1a03`, the character sheet.
    ///
    /// Reached from both prompts and from both endings: `1000:ec89` (the
    /// street `\` prompt's `s`, compared at `1000:ec82`), `1000:4c35` (the
    /// `Битва\` prompt's `s`, compared at `1000:4c2e`), `1000:ee36` (the
    /// quit tail) and `1000:512b` (the rector-victory ending). All four
    /// render the same sheet, because the function takes no arguments --
    /// see [`Game::sheet_kit`].
    ///
    /// The lines are built by [`crate::character_sheet`] so a test can
    /// assert them; this method is only the `term::println` loop the
    /// original's 50 `Write`/`WriteLn` calls collapse into.
    fn show_stats(&self) {
        for line in character_sheet::lines(&self.player, &self.player.name, &self.sheet_kit()) {
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
        term::println(&text::fill(
            "Урон #-#",
            &[enemy.dmg_min as i64, enemy.dmg_max as i64],
        ));
        term::println(&text::fill(
            "Здоровье #/#  ",
            &[enemy.hp as i64, enemy.hpmax as i64],
        ));
        term::println(&text::fill("^2Броня #    ", &[enemy.armor as i64]));
    }

    /// `v` at the STREET prompt: the original does nothing at all, so
    /// neither does this.
    ///
    /// **Established from flow.** `v` is compared at exactly **one** site in
    /// the whole image, `1000:4caa`, and that site pushes the *fight*
    /// prompt's buffer `20ae:3a72`. `entry`'s chain -- `crate::commands`'
    /// module doc lists it in full -- never compares `v` against
    /// `20ae:3972`.
    ///
    /// The scan behind that is a closure, not a list: every `9a d8 0b 78 0f`
    /// (`rtl_str_compare`) call in `orig/g.exe` is **75** sites, and each
    /// one's token is read out of its own `mov di,<token>` / `push cs` /
    /// `push di` setup rather than inferred. Sixty-six match that shape and
    /// exactly one of them carries `v`. The nine that do not were read
    /// individually, because a completeness claim that skips what its pattern
    /// missed is the failure `docs/re/METHODOLOGY.md` names: eight are
    /// `FUN_1000_29c4`'s own `h`/`mh` compares (`1000:29f5`, `1000:2a07`,
    /// `1000:2a6f`, `1000:2aa5`, `1000:2af7`, `1000:2b45`, `1000:2b8e`,
    /// `1000:2bb5`), which push the stack local at `[bp-0x100]` instead of a
    /// fixed buffer, and the ninth is `1000:75f6`, the `y` at CS `0x74a9` in
    /// `FUN_1000_6a0d`. None of the nine is a `v`.
    ///
    /// This method used to print `^4Ни кто не хочет за тебя впрягаться.`
    /// (CS `0x35e9`). That line is real, but it belongs to the fight prompt's
    /// `v` arm at `1000:4d0a`, where it is the *cred too low* refusal --
    /// see [`Game::backup_in_fight`]. Printing it here made the street
    /// answer a verb the original leaves unanswered.
    fn call_backup(&self) {}

    /// `f` at the STREET prompt -- `1000:ec96`..`1000:ecbd`.
    ///
    /// **Established from flow**, re-derived from an aligned walk out of
    /// `entry`:
    ///
    /// ```text
    /// ec96  call 0f78:0bd8            ; the `f` token, CS 0xaa4c
    /// ec9b  jnz 0xecbd
    /// ec9d  cmp byte [0x394d],0
    /// eca2  jz 0xecbd                 ; NO pistol -> nothing is printed
    /// eca4  mov di,0xaa4e             ; ^6Ты чё псих? мигом менты накроют!
    /// ```
    ///
    /// So the refusal is what the game says to someone who is **carrying** a
    /// pistol on the street; without one the verb is accepted and answered
    /// with silence. This method used to print it unconditionally, with a doc
    /// comment admitting the gating "is not tracked by
    /// `crate::model::Fighter`" -- it is [`Game::pistol`] now.
    ///
    /// Nothing else happens either way: no draw, no state change, and the
    /// pistol is not fired. `1000:ecbd` is the next verb's compare.
    fn shoot(&self) {
        if self.pistol.owned {
            term::println("^6Ты чё псих? мигом менты накроют!");
        }
    }

    /// `w`/`run` -- one whole wander turn, the complete `Random` sequence
    /// included.
    ///
    /// **Established from flow.** `1000:ae86` (`w`) and `1000:ae97` (`run`)
    /// both jump to `1000:aea1`; there is exactly one wander path, and
    /// `1000:ae63`'s `ReadLn` into `DS:3972` is the main loop's own read,
    /// which this port's `run()` mirrors. A turn is
    /// [`Game::wander_preamble`] (`1000:aea1`..`1000:b3b9` -- fourteen
    /// catalogued `Random` sites and the state steps between them) followed
    /// by the bucket dispatch at `1000:b3ba`.
    ///
    /// **This function used to spend exactly one draw where the original
    /// spends nine.** The bucket roll is draw 12 of 14, so the port's stream
    /// desynchronised from the original's on the first walk and never
    /// recovered. Task 11c wired the rest of the sequence in;
    /// `tests/wander_sequence.rs` replays five captured runs of the original
    /// (`data/rng_trace.json`) against it draw for draw.
    ///
    /// The dispatch at `1000:b3ba` reads `20ae:3970` and compares it against
    /// 1 (`1000:b3bd`), 2 (`1000:b4e8`) and 3 (`1000:b5ae`), falling through
    /// to bucket 4 at `1000:b836`:
    ///
    /// * **0** -- no arm matches, so the turn ends with nothing. The only
    ///   way to reach it is the church, which zeroes the already-rolled
    ///   bucket at `1000:8282`.
    /// * **1** (`1000:b3c4`) -- toggles `20ae:3693` and writes one
    ///   district-keyed line from either of two sets (`1000:b3db..`,
    ///   `1000:b465..`). The **toggle is modelled** (see
    ///   [`Game::flag_3693`] -- `FUN_1000_0d14` branches on it twice, so it
    ///   changes both the draw count and the draw values of every later
    ///   encounter); the lines are not, neither set having been extracted.
    ///   The bucket spends no draw either way. See `docs/re/gaps.md`.
    /// * **2** (`1000:b4ef`) -- the girl encounter, [`Game::wander_girl`].
    /// * **3** (`1000:b5b5`) -- the fight encounter, below.
    /// * **4** (`1000:b836`) -- flavour only, branching on the joint buff's
    ///   countdown `20ae:38cd`. **Not modelled**, same reason as bucket 1.
    ///
    /// The fight encounter, `1000:b5b5` onward:
    ///
    /// * `1000:b5b8` -- `call FUN_1000_0d14` with `param_1 = 0`, which rolls
    ///   the whole opponent record at `20ae:3952`. Recovered in Task 11f:
    ///   [`Game::roll_enemy`].
    /// * `1000:b5c0` -- `cmp word [0x3952],8` / `jnz 0xb5ca`. A rolled
    ///   `Мент` skips everything below and takes [`Game::cop_encounter`]
    ///   instead, which asks no question at all.
    /// * `1000:b5ed`/`1000:b5f1` -- `Random(district * 7 + 15)`, halved
    ///   first when [`Game::prison_tattoo`] is set (`1000:b5da`
    ///   `cmp byte [0x38bc],1`). `1000:b5fc`..`1000:b61b` then compares the
    ///   player's luck against it as a longint and picks between the two
    ///   answer blocks below: the class threshold is 3 when luck lost the
    ///   compare (`1000:b60a`) and 7 when it won (`1000:b614`).
    /// * `1000:b6a6`..`1000:b6dd` -- the aggressive block. Writes `^6Идет `
    ///   (file `0xA267`), the rank name, and ` # уровня, ищущий кого
    ///   отпинать. Хочешь наехать?` (file `0xA28A`) as one `WriteLn`.
    ///   **No prompt is written after it** -- the very next instruction
    ///   (`1000:b6e0`) sets up the `ReadLn`. `1000:b61e`..`1000:b65b` is the
    ///   quiet block: same shape, but file `0xA26F`
    ///   (` # уровня. Хочешь наехать?`) and no decline roll.
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
    /// The `1000:b691` block's decline arm has **no** random roll: a non-`y`
    /// answer simply ends the encounter. An earlier revision of this comment
    /// said which of the two blocks a real encounter reaches "has not been
    /// traced" and that this port "always takes the `Random(2)` branch".
    /// Both are now false: `1000:b5fc` is the luck-versus-`1000:b5f1`
    /// compare described above, and the port takes whichever block it
    /// selects. The 11 stops at `1000:b5f1` against only 2 at `1000:b725` in
    /// `data/rng_trace.json` are the live confirmation that the quiet block
    /// is the common one.
    ///
    /// `pub` so `tests/wander_sequence.rs` can drive one turn at a time;
    /// `run()` is still the only path a player takes.
    pub fn walk(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        let bucket = self.wander_preamble(lines)?;
        match bucket {
            // 1000:b3c4..1000:b3ce -- bucket 1's only lasting effect. The
            // two line sets it writes are still not extracted (see
            // `docs/re/gaps.md`), but the toggle itself is not optional:
            // `FUN_1000_0d14` branches on it twice.
            1 => {
                self.flag_3693 = !self.flag_3693;
                return Ok(());
            }
            2 => return self.wander_girl(lines),
            3 => {}
            // 0 (church-cancelled) and 4: nothing this port models.
            _ => return Ok(()),
        }

        // 1000:b5b5/1000:b5b8 -- `mov al,0` / `push ax` / `call 0xd14`.
        let enemy = self.roll_enemy(0);
        // 1000:b5bb -- `c6 06 72 3b 00`, the accept flag cleared first.
        // 1000:b5c0 -- `cmp word [0x3952],8` / `jnz 0xb5ca`; the cop gets its
        // own block with no prompt and no `Хочешь наехать?`.
        if enemy.class == 8 {
            return self.cop_encounter(enemy, lines);
        }

        // 1000:b5ca..1000:b5f1 -- `district * 7 + 15`, halved by the tattoo,
        // is the notice roll's `n`.
        let mut n = u16::from(self.district) * 7 + 15;
        if self.prison_tattoo {
            n /= 2;
        }
        let notice = self.rng.below_at("1000:b5f1", n);
        // 1000:b5fc..1000:b61b -- luck is compared against it as a longint
        // (`cwd`, then `cmp dx,bx` / `cmp ax,cx` / `jnc 0xb614`), and the
        // class threshold differs between the two arms: 3 when luck lost the
        // compare (`1000:b60a`), 7 when it won (`1000:b614`).
        //
        // Not a like-for-like widening: at file 0xcec6 (`xor dx,dx`) the
        // original zero-extends `notice` into `dx:ax`, but at file
        // 0xcecc..0xcecf (`mov ax,[0x38a4]` / `cwd`) it *sign*-extends
        // `luck`. This port widens both sides the same way, via
        // `i32::from(u16)` (zero-extension), so it never reproduces the
        // negative interpretation the original's `cwd` would give a `luck`
        // value with bit 15 set. Unreachable at realistic luck values (never
        // near 0x8000), but the port is wider than the original here.
        let aggressive = if i32::from(self.player.luck) < i32::from(notice) {
            enemy.class >= 3
        } else {
            enemy.class >= 7
        };
        term::print("^6Идет ");
        term::print(&enemy.name);
        // 1000:b644 (file 0xA26F) vs 1000:b6c8 (file 0xA28A).
        term::println(&text::fill(
            if aggressive {
                " # уровня, ищущий кого отпинать. Хочешь наехать?"
            } else {
                " # уровня. Хочешь наехать?"
            },
            &[enemy.level as i64],
        ));
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if answer.trim().eq_ignore_ascii_case("y") {
            self.run_combat(enemy, lines)?;
        } else if !aggressive {
            // 1000:b696 -- the quiet arm has no decline roll at all: a
            // non-`y` answer simply ends the turn.
        } else if self.rng.below_at("1000:b725", 2) == 0 {
            term::println("^4Он тебя заметил.");
            self.run_combat(enemy, lines)?;
        } else {
            term::println("^2Ты смылся.");
        }
        Ok(())
    }

    /// `1000:b76a`..`1000:b81a` -- what a rolled class 8 (`Мент`) does
    /// instead of the ordinary encounter.
    ///
    /// **Established from flow**, re-derived from `orig/g.exe` disassembling
    /// forward from `1000:b353` (the `9a 4b 11 78 0f` at file `0xcc23`, the
    /// wander's own bucket roll) so every address below is on a confirmed
    /// instruction boundary:
    ///
    /// * `1000:b76a`..`1000:b77f` -- writes `^6Идет ментяра # уровня гроза гопов.` (file `0xA2DB`) with `[0x395c]`, the rolled level, pushed at
    ///   `1000:b76f`. **No line is read**: there is no "Хочешь наехать?" on
    ///   this path.
    /// * `1000:b784`..`1000:b792` -- `district * 7 + 15` (`mul dx` with
    ///   `dx = 7`, then `add ax,0xf`) pushed into `Random`. Unlike
    ///   `1000:b5ed`'s roll this one is **never** halved by the tattoo --
    ///   there is no `cmp byte [0x38bc],1` between `1000:b784` and the call.
    /// * `1000:b79d`..`1000:b7a9` -- luck as a longint against it;
    ///   `jc 0xb7c6` is taken when luck is **below** the roll.
    /// * luck won -> `1000:b7ab` writes `^2Ты затаился, прикинулся не
    ///   гопом... Мент вроде не заметил` (file `0xA300`) and leaves the
    ///   accept flag clear.
    /// * luck lost and `[0x38b3]` is 1 -> `1000:b7cd`/`1000:b7e6` write the
    ///   тёмные очки pair (files `0xA33C`, `0xA38A`); still no fight.
    /// * luck lost without them -> `1000:b801` writes `^4Запалил!` (file
    ///   `0xA3B2`) and `1000:b81a` (`c6 06 72 3b 01`) sets the accept flag,
    ///   so `1000:b829` calls `FUN_1000_3d11` with `param_1 = 0`.
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
        let notice = self.rng.below_at("1000:b792", n);
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
        self.run_combat(enemy, lines)
    }

    /// `1000:aea1`..`1000:b3b9`: everything a walk does before the bucket
    /// dispatch, in execution order. Returns the value `20ae:3970` holds
    /// when `1000:b3ba` reads it.
    ///
    /// The order, every `n`, every gate and every state step come from
    /// `data/wander.json`'s `steps` array (prose and addresses:
    /// `docs/re/wander.md`), which carries the opcode bytes at each address
    /// cited below so a five-byte drift is checkable without a disassembler.
    /// All fourteen sites were re-derived from `orig/g.exe` for this
    /// implementation, and all fourteen have been observed firing in the
    /// running original with the `n` used here (`data/rng_trace.json`).
    ///
    /// Two shapes are easy to get wrong and are called out where they occur:
    ///
    /// * **Draws 1 and 2 are not one-shots.** Their never-repeat flag is
    ///   written at `1000:af71`/`1000:afd0`, *after* the `or ax,ax / jnz` at
    ///   `1000:af6d`/`1000:afcc`, so the flag is set only by the 1-in-20
    ///   roll that actually returns `0`. Until then the draw fires every
    ///   turn. Steady state is nine draws, decaying to eight and then seven.
    /// * **Draws 5..8 always fire.** Only their *effect* is gated on the
    ///   discovery flag still being clear; the roll happens either way.
    fn wander_preamble(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<u8> {
        // seq 1, 1000:aea1 -- the joint buff decays, and hitting zero takes
        // back exactly what 1000:4b57 granted (1000:aeb3/aeb8/aebc).
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

        // seq 2, 1000:aeda -- `run` (and only `run`) writes file 0x9D7D
        // here. `crate::commands::parse` folds `w` and `run` into one
        // `Command::Walk`, so this port cannot tell them apart and writes
        // nothing. It costs no draw; recorded in `docs/re/gaps.md`.

        // seq 3, 1000:af04 -- the den's loan credit tops up once per walk
        // while it is below district*10 (`jnl 0xaf1d` skips otherwise).
        if u16::from(self.den_loan_credit) < u16::from(self.district) * 10 {
            self.den_loan_credit += 1;
        }

        // seq 4, 1000:af1d -- the dealers' 25-walk delivery counter. Three
        // gates before the increment (1000:af1d, af24, af2b), then the call
        // only on the turn it becomes exactly 25 and only with a phone.
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

        // Draw 1, 1000:af68 -- Random(20), gate `[0x3b78] == 0` at
        // 1000:af5d. The flag is set at 1000:af71 BEFORE the den/phone tests
        // at 1000:af76/1000:af7d, so a player without a phone loses the
        // errand permanently and sees nothing.
        if !self.den_errand_1_pending && self.rng.below_at("1000:af68", 20) == 0 {
            self.den_errand_1_pending = true;
            if self.places.is_found(Location::Den) && self.has_mobile {
                // 1000:af84..1000:afb7 concatenates three pieces into one
                // WriteLn: file 0x9DDB, the name at DS:379c, file 0x9DEA.
                term::print("Телефон:^6Алё,");
                term::print(&self.player.name);
                term::println("^6? ты где щас? Тут помощь нужна.(Иди в притон)");
            }
        }

        // Draw 2, 1000:afc7 -- the same shape one flag along, with понтовость
        // >= 100 as an extra print gate (1000:afdc).
        if !self.den_errand_2_pending && self.rng.below_at("1000:afc7", 20) == 0 {
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

        // 1000:b022 and 1000:b0ce are two separate `cmp byte [0x38bb],1`
        // gates; without a phone the first jumps past draw 3 to 1000:b0ce
        // and the second jumps past draw 4 AND the two cooldown messages
        // straight to 1000:b16c.
        if self.has_mobile {
            // Draw 3, 1000:b030 -- Random(200), the wrong-number gag. The
            // original spaces these with 0f16:031a `ReadKey`s (not a delay
            // -- docs/re/rtl.md:494; `Delay` is the unrelated 0f16:02a8),
            // waiting for a keystroke between each message; this site does
            // not port that wait. [`Game::enter_district_5`] shows the port
            // DOES have a working substitution for a `ReadKey` (a discarded
            // line read, the same trick `src/persist.rs`'s `choose_slot`
            // uses) -- it just is not applied to these phone-call gags. See
            // docs/re/gaps.md.
            if self.rng.below_at("1000:b030", 200) == 0 {
                term::println("Телефон:^6Алё Вася?");
                term::print("^2Нет это ");
                term::print(&self.player.name);
                term::println(".");
                term::println("Телефон:^6А Васю можно?");
                term::println("^2Нет, он будет в больнице в ближайшие 2 месяца.");
            }
            // Draw 4, 1000:b0dc -- Random(100); prints only with a girl.
            if self.rng.below_at("1000:b0dc", 100) == 0 && self.places.is_found(Location::Girl) {
                term::println("Телефон(Твоя пассия):^5Привет, это я. Зайдешь ко мне сегодня?");
                term::println("^2А ты: Безбазаров, жди.");
            }
            // seq 9/10, 1000:b11e and 1000:b145 -- the "it blew over" calls,
            // on the last turn of each ban and only with the den known.
            //
            // CURRENTLY UNREACHABLE. Both countdowns are permanently 0 in
            // this port: no `mov byte [0x3b76],5` (1000:c465), no
            // `mov byte [0x3b77],5` (1000:e23e), and no gate at 1000:b95e /
            // 1000:df1a. Kept at the right addresses and in the right order
            // so that implementing those three closes the gap in one place --
            // see docs/re/gaps.md, "The two ban countdowns are modelled and
            // decremented but never set".
            if self.market_ban_countdown == 1 && self.places.is_found(Location::Den) {
                term::println(
                    "Телефон:^2Это ты там на базаре шухер наводил? Ну короче там менты свалили.",
                );
            }
            if self.club_ban_countdown == 1 && self.places.is_found(Location::Den) {
                term::println("Телефон:^2Ты че там, в клуб-та пойдёшь. Уже утряслось всё.");
            }
        }

        // seq 11, 1000:b16c/1000:b177 -- both cooldowns tick down
        // (`fe 0e 76 3b` / `fe 0e 77 3b`). Dead for the same reason as the
        // two branches above: nothing sets either field non-zero.
        if self.market_ban_countdown > 0 {
            self.market_ban_countdown -= 1;
        }
        if self.club_ban_countdown > 0 {
            self.club_ban_countdown -= 1;
        }

        // Draws 5..8 -- the four discovery rolls. 1000:b186 Random(10) vet,
        // 1000:b1b8 Random(10) market, 1000:b1ea Random(100) club,
        // 1000:b21c Random(100) gym. The comparison constants ARE the
        // probabilities (`docs/re/METHODOLOGY.md`).
        if self.rng.below_at("1000:b186", 10) == 0 && !self.places.is_found(Location::Vet) {
            self.places.mark_found(Location::Vet); // 1000:b196
            term::println("^1Ты спросил у прохожего где больница.");
        }
        if self.rng.below_at("1000:b1b8", 10) == 0 && !self.places.is_found(Location::Market) {
            self.places.mark_found(Location::Market); // 1000:b1c8
            term::println("^1Ты нашел базар.");
        }
        if self.rng.below_at("1000:b1ea", 100) == 0 && !self.places.is_found(Location::Club) {
            self.places.mark_found(Location::Club); // 1000:b1fa
            term::println("^1Ты увидел объявление \"Типа заходи в наш понтовый клуб\".");
        }
        if self.rng.below_at("1000:b21c", 100) == 0 && !self.places.is_found(Location::Gym) {
            self.places.mark_found(Location::Gym); // 1000:b22c
            term::println("^1На стене реклама \"Жизнь тяжела. Если не хочешь сдохнуть качайся!\".");
        }

        // seq 16 + draw 9, both behind `[0x38c1] != 0` at 1000:b24a -- the
        // ring "Господи помилуй", whose own description string (file
        // 0x53DD) advertises exactly this: +3 HP and a 5% fracture heal.
        if self.ring_gospodi_pomilui {
            // 1000:b251..1000:b26b, clamped to hpmax.
            if self.player.hp < self.player.hpmax {
                self.player.hp += 3;
                if self.player.hp > self.player.hpmax {
                    self.player.hp = self.player.hpmax;
                }
            }
            // Draw 9, 1000:b272 -- Random(20); 1000:b279 `ja` means only a
            // zero continues. At most ONE fracture clears, jaw first: the
            // leg block at 1000:b289 is reached only when the jaw is intact
            // (`jnz 0xb2a7` at 1000:b280), and the jaw block at 1000:b2ae
            // only when it is broken.
            if self.rng.below_at("1000:b272", 20) == 0 {
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

        // seq 18..22, 1000:b2cc -- the class-perk dispatch. Every arm
        // converges on 1000:b34d.
        match self.player.class {
            // 1000:b2cf -- Отморозок heals one scratch a walk, the
            // "Бонус - Самолечение царапин" the creation menu advertises.
            4 => {
                if self.player.hp < self.player.hpmax {
                    self.player.hp += 1;
                }
            }
            // 1000:b2e3 -- Гопник has no wander perk.
            5 => {}
            // 1000:b2ea -- Вор steals, the menu's "Бонус - Воровство".
            6 => {
                // Draw 10, 1000:b2fa. `n` is built at 1000:b2ef..1000:b2f8
                // as district * 20 (`mov dx,0x14` / `mul dx`).
                let r = self
                    .rng
                    .below_at("1000:b2fa", u16::from(self.district) * 20);
                // 1000:b305..1000:b311: luck is sign-extended (`cwd`) and
                // the result zero-extended, and the theft succeeds when
                // luck >= result.
                if i32::from(self.player.luck) >= i32::from(r) {
                    // Draw 11, 1000:b321. `n` is district * 5, built at
                    // 1000:b313..1000:b31e as (district << 2) + district.
                    let amount = self.rng.below_at("1000:b321", u16::from(self.district) * 5) + 1;
                    // 1000:b326/1000:b32d: [0x3b74] := r + 1, money += it.
                    self.player.money += i32::from(amount);
                    term::println(&text::fill(
                        "^2Опа бабки! # рублей на пиво!",
                        &[amount as i64],
                    ));
                }
            }
            _ => {}
        }

        // Draw 12, 1000:b353 -- the bucket roll. `n` is built at 1000:b34d
        // as `mov ax,5` / `imul ax`, i.e. AX*AX = 25, and 1000:b358 stores
        // r+1 (so 1..25) into 20ae:3971. The chain at 1000:b35c..1000:b393
        // tests the highest boundary first.
        let roll = self.rng.below_at("1000:b353", 25) + 1;
        let mut bucket = if roll >= 10 {
            4
        } else if roll >= 5 {
            3
        } else if roll >= 2 {
            2
        } else {
            1
        };

        // Draw 13, 1000:b39e -- Random(200); a zero calls the church at
        // 1000:b3a7.
        if self.rng.below_at("1000:b39e", 200) == 0 {
            self.church();
            // 1000:8282 `c6 06 70 39 00` is the routine's last act before
            // its single epilogue and no jump inside it targets an address
            // above that, so EVERY path zeroes the bucket: a church turn
            // produces no encounter even though the roll already happened.
            bucket = 0;
        }

        // Draw 14, 1000:b3ae -- Random(100); a zero calls the mage at
        // 1000:b3b7. It spends no draw but does block on a ReadLn.
        if self.rng.below_at("1000:b3ae", 100) == 0 {
            self.mage(lines)?;
        }

        Ok(bucket)
    }

    /// The church, `1000:7c67`..`1000:82af` -- one procedure, one prologue,
    /// one epilogue (`89 ec 5d c3` at `1000:82af`), called from exactly one
    /// site (`1000:b3a7`).
    ///
    /// Three sermon arms are selected by `20ae:3951` (`== 2` at `1000:7c76`,
    /// `== 1` at `1000:7ceb`, `== 0` at `1000:7dcb`) and all converge on
    /// `1000:7f5f`, so draw 15 is unconditional once the church fires. The
    /// two lower arms raise the stage on their way out (`1000:7dc7`,
    /// `1000:7f5b`), which is why it saturates at 2.
    ///
    /// **Not reproduced:** the two long sermons (the `== 0` and `== 1`
    /// arms), and the old/new rank names the level-up arm prints from the
    /// `DS:0b42` 256-byte-stride table. Both are text only and cost no
    /// draw; recorded in `docs/re/gaps.md`.
    fn church(&mut self) {
        let stage = self.church_visits;
        if stage <= 1 {
            self.church_visits += 1;
        } else {
            // The `== 2` arm's four lines, files 0x904C/0x9083/0x909F/0x90B4.
            term::println("Бродя по окрестностям с самыми грязными намериниями...");
            term::println("Ты наткнулся на храм Божий.");
            term::println("^1Бог: \"А ты опять.\"");
            term::println("^1Ну ладно насылаю на тебя \"благославление\"");
        }

        // Draw 15, 1000:7f63 -- Random(5). Five equally likely arms.
        match self.rng.below_at("1000:7f63", 5) {
            // 1000:7f68's zero arm: a forced level-up.
            0 => {
                term::println("^1Да увеличится твоя понтовость!");
                // 1000:7fe4/1000:7fe7 `mov ax,[0x38d0]` / `mov [0x38ce],ax`
                // -- xp := threshold -- then `mov al,0` / `call 0x2526`.
                // 1000:2526's entry test (1000:2535..1000:253c) therefore
                // passes by construction, its xp loop runs exactly once, and
                // the per-level body spends draws 17 and 18 at 1000:25fe
                // (loop bound `cmp word [bp-0x8],0x2` at 1000:287d). At
                // level 40 (1000:2580) it spends no draw and grants nothing,
                // but the xp rewrite above has already happened.
                self.progress.xp = self.progress.threshold;
                progress::apply_levels(
                    &mut self.progress,
                    &mut self.player,
                    &mut self.rng,
                    0,
                    false,
                );
            }
            // 1000:7ff3 `cmp ax,1` / 1000:7ff6 `jz 0x7ffb` -- a stat
            // blessing. (1000:7f68 is the ZERO arm above, not this one.)
            1 => match self.rng.below_at("1000:7fff", 4) {
                // 1000:8022..1000:8043. The dmg_min term reads the
                // ALREADY-incremented strength, so it is +1 when the new
                // strength is even -- the same rule as a level-up's.
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
                    // 1000:808b..1000:8094
                    self.player.vitality += 1;
                    self.player.hpmax += 5;
                    self.player.hp += 5;
                }
                _ => {
                    term::println("^1Да снизойдет на тебя удача!");
                    self.player.luck += 1; // 1000:80b9
                }
            },
            // 1000:80c0's `cmp ax,2` -- the first unfired one-shot gift.
            // These are the same three flags the post-kill block grants
            // (`docs/re/progression.md`); this is a second grant site.
            2 => {
                term::println("^1Дарю тебе феньку!");
                if !self.oneshot_gift_1 {
                    // 1000:80e1 gate, 1000:8101..1000:8134.
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
                    // 1000:813c gate, 1000:815c..1000:8184.
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
                    // 1000:818b gate, 1000:81c4. Text only here -- the
                    // ring's effect is the wander regen and draw 9 above.
                    term::println("^1Ваще полезное кольцо \"Господи помилуй\"");
                    term::println("^1Восст. жизни - 3, 5% - самозарост переломов");
                    self.ring_gospodi_pomilui = true;
                }
            }
            // 1000:81cb's `cmp ax,3` -- `inc byte [0x38b2]` at 1000:81e9.
            // 20ae:38b2 is fighter-record offset +0x16, which
            // `crate::model` and `docs/re/combat.md` already establish as
            // ARMOUR (subtracted from damage at 1000:4769, printed as
            // `^2Броня #` at 1000:163f) -- and "накладываю защиту" is
            // exactly that. Corroborated by state: `SAVE_R3.SAV` holds 4 at
            // `.SAV 0x216` and run E's guest reports 4 there (the probe
            // captures still spell that column `unk_38b2`).
            3 => {
                term::println("^1Накладываю на тебя защиту!");
                self.player.armor += 1;
            }
            // 1000:81ef's `cmp ax,4` -- 1000:820d..1000:821a.
            _ => {
                term::println("^1Да увеличится, офигенно, твоя понтовость среди гопоты!");
                let gain = i32::from(self.district) * 50 + 50;
                self.pontovost_street += gain;
                term::println(&text::fill("^1Получи #!", &[gain as i64]));
            }
        }

        // 1000:8247 `cmp byte [0x3951],0x2` / `jnc 0x8269`, read AFTER the
        // stage was raised.
        if self.church_visits < 2 {
            term::println("^1А теперь вали отсюда и никогда здесь не появляйся!");
        } else {
            term::println("^1А теперь проваливай!");
        }
    }

    /// The wandering mage Рушель Блаво, `1000:7538`..`1000:7778`, called
    /// only from `1000:b3b7`. **It contains no `Random` call** -- it spends
    /// no draw, but it does block on a `ReadLn`, so it consumes a line.
    ///
    /// `1000:75c7`..`1000:75d1` reads into a **stack local** `[bp-0x100]`,
    /// neither `DS:3972` nor `DS:3a72` -- a third input buffer -- then
    /// `1000:75e6` case-folds it through `0eed:0216` and `1000:75f6`
    /// compares it against the token `y` (file `0x8D79`, `01 79`).
    ///
    /// **A divergence inside the original, reproduced here.** The price it
    /// PRINTS is `district * 25` (`1000:758d`, `ba 19 00`); the price it
    /// CHECKS and CHARGES is `district * 50` (`1000:7605` and `1000:7618`,
    /// both `ba 32 00`, debit at `1000:761d`).
    ///
    /// **The two file writes on the paid path are reproduced** (Task 19):
    /// the 694-byte record into `save_r0.sav` (`1000:764e`/`1000:765d`), the
    /// seven discovery flags into `places.sav` (`1000:766f`..`1000:7724`),
    /// and `^0Сохранено! ^1Можешь беспредельничать дальше.` (`1000:7729`,
    /// file `0x8D92`) -- see [`Game::mage_save`](crate::persist). They used
    /// to be out of reach because `Save::parse` was the only constructor and
    /// `.SAV` `0x214`/`0x2ae` were unknown; both spans are established now.
    ///
    /// A write that fails is reported and the turn continues. The original
    /// has no failure message on this path at all: `1000:761d` debits before
    /// the file is opened and nothing after it tests `IOResult`, so the
    /// money leaves either way. Printing the host error is a PORT DECISION
    /// -- silently swallowing an I/O failure would be worse than one line
    /// the original never prints.
    /// `pub` for the same reason [`Game::walk`] is: it is the only way a
    /// test can reach this arm. Wander bucket 14 is what dispatches it in
    /// play (`1000:b3b7`), and forcing that bucket needs a seed the binary
    /// does not take.
    pub fn mage(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        term::println("Бродя по окрестностям с самыми грязными намериниями...");
        term::println("Ты встретил великого мага и экстрасенса - Рушеля Блаво.");
        term::println(&text::fill(
            "За # рублей он может сделать сохранение прямо здесь.",
            &[i64::from(self.district) * 25],
        ));
        term::println("Ты хочешь сохраниться?");
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            // 1000:775f, file 0x8DDC.
            term::println("^6Нехотите как хотите - мое дело предложить");
            return Ok(());
        }
        let price = i32::from(self.district) * 50;
        if self.player.money < price {
            // 1000:7744, file 0x8DC1.
            term::println("^6Парень, все стоит бабок!");
            return Ok(());
        }
        self.player.money -= price;
        // 1000:7621..1000:773d. The debit above is 1000:761d, and it happens
        // BEFORE the file is opened in the original too.
        if let Err(e) = self.mage_save() {
            term::println(&format!("^6{e}"));
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
        if self.rng.below_at("1000:b54e", 2) == 0 {
            term::println("^5Ты такой подкатываешь, а она:\"Глянулся ты мне парниша\"");
            self.places.mark_found(Location::Girl);
        } else {
            term::println(
                "^4Ты ещё подкатить неуспел - а она:\"Отдыхай урод\". - Тебя обломали кент",
            );
        }
        Ok(())
    }

    /// Turbo Pascal's `Round` of a value that is an exact multiple of one
    /// half, taking that value **doubled** so the caller needs no float.
    ///
    /// **Established from flow.** `Round` is `0f78:1131` (`b5 01`
    /// `mov ch,1`, then `call 0f78:1091`); `0f78:1129` is `Trunc` and
    /// differs only in `ch`. Inside the worker, `0f78:10d0`
    /// (`0a ed` `or ch,ch`) selects the rounding tail: `0f78:10d4`
    /// `02 ff` (`add bh,bh`) sets CF when the byte shifted out of the
    /// mantissa is >= 0x80 -- i.e. when the fraction is >= 1/2 -- and
    /// `15 00 00` / `83 d2 00` (`adc ax,0` / `adc dx,0`) adds one to the
    /// **magnitude**; the sign is only applied afterwards at `0f78:10e4`.
    /// So it is round-half-away-from-zero, not half-to-even.
    ///
    /// That distinction is load-bearing exactly once: `1000:0e6c`'s
    /// `level * 1.5`. Run A turn 11 of `data/rng_trace.json` rolls level 1
    /// there and then spends 12 draws at `1000:0efd` rather than 10, which
    /// only happens if `Round(1.5)` is 2.
    fn round_half(twice: i32) -> i32 {
        if twice >= 0 {
            (twice + 1) / 2
        } else {
            -((-twice + 1) / 2)
        }
    }

    /// `FUN_1000_0d14` (`1000:0d14`..`1000:11bf`, file `0x25e4`..`0x2a8f`) --
    /// the random-encounter opponent, rolled into the record at `20ae:3952`.
    ///
    /// **Established from flow.** The whole routine was disassembled with
    /// `ndisasm -b16 -o 0xd14 -e 0x25e4 orig/g.exe`, i.e. from the routine's
    /// own `55` / `89 e5` (`push bp` / `mov bp,sp`) entry, and every one of
    /// its fourteen `Random` call sites carries the `9a 4b 11 78 0f`
    /// signature at the address cited beside it below. All fourteen fire in
    /// the running original with the `n` used here: `data/rng_trace.json`'s
    /// `sites_not_in_catalogue` records 13 stops each (5 for `1000:0d91`,
    /// 348 for the `1000:0efd` loop), and this implementation reproduces
    /// every one of them in order.
    ///
    /// `param_1` is `[bp+4]`, a byte. `1000:b5b8` -- the wander encounter,
    /// the only caller this port has -- passes 0; `1000:c3d0`, `1000:dc0e`
    /// and `1000:e181` pass 1 and `1000:ddf6` passes 2. The clamps at
    /// `1000:0da7` and `1000:0dba` are the only thing it selects.
    ///
    /// Step by step, with the addresses:
    ///
    /// 1. `1000:0d22`..`1000:0d68` -- `Random(0x33) + 1`, folded by a
    ///    triangular walk: for `i` in `1..=10`, if the running value is
    ///    negative after subtracting `i` the class is `10 - i` and the walk
    ///    stops, otherwise `i` is subtracted and the walk continues
    ///    (`1000:0d64` `cmp byte [bp-1],0x0a` / `jnz 0xd35` is the bound).
    ///    51 can never survive all ten subtractions (they total 55), so the
    ///    walk always leaves through the break. It maps low rolls to high
    ///    classes: `Random(0x33)` of 0..1 gives class 8, 44..50 gives class 0.
    /// 2. `1000:0d6a`..`1000:0d83` -- plus `Random(district)`.
    /// 3. `1000:0d86`..`1000:0d96` -- plus `Random(4)`, but **only** when
    ///    `[0x3693]` is set (see [`Game::flag_3693`]).
    /// 4. `1000:0d9a`..`1000:0dc4` -- clamp to 9; then `param_1 == 1` clamps
    ///    to 7 and `param_1 == 2` forces 8.
    /// 5. `1000:0dc6`..`1000:0e45` -- крутизна:
    ///    `Round(player_level * f / d + s - 2) + 4 * Random(district)`,
    ///    where `s` is `Random(5)` (`1000:0ddd`), `f` is `Random(2) + 1`
    ///    (`1000:0df0`) and `d` is `Random(2) + 1` (`1000:0e04`). The
    ///    multiply is `0f78:09d2` (32-bit `imul`), the divide is
    ///    `0f78:1117` (the real-divide entry thunk: 10 bytes at
    ///    `0f78:1117`..`1120` that check `cl` for a zero divisor and raise
    ///    runtime error 200 out of line at `0f78:1145`), the add and
    ///    subtract are `0f78:10ff`/`0f78:1105`, and `0f78:1131` rounds.
    ///    `1000:0e48` floors it at 0, and `1000:0e54`..`1000:0e76` then
    ///    multiplies it by **1.5** (`0f78:1111`, real multiply, against the
    ///    constant `ax=0x0081 bx=0 dx=0x4000`) when `[0x3693]` is set.
    /// 6. `1000:0e79`..`1000:0e8a` -- the four stats are zeroed.
    /// 7. `1000:0e8d`..`1000:0fee` -- `sum(weights) + крутизна * 2` points,
    ///    each `Random(sum(weights)) + 1` (`1000:0efd`) bucketed against the
    ///    running prefix sums of the class's weight row at `20ae:0002 +
    ///    class*4` (`mov di,[0x3952]` / `shl di,1` / `shl di,1` /
    ///    `mov al,[di+0x2..0x5]`), i.e. `crate::progress::CLASS_WEIGHTS`.
    ///    Both the sum and the point count are stored as **bytes**
    ///    (`1000:0ed1` `mov [bp-2],al`, `1000:0ee2` `mov [bp-4],al`).
    ///
    ///    An earlier reading, recorded in `docs/re/tables.md`, had this loop
    ///    drawing `Random(remaining points)`. It does not: the `n` is the
    ///    constant weight-row sum, which is why the observed `n` set at
    ///    `1000:0efd` is exactly `{6, 8, 9, 12, 20, 22}` -- the six distinct
    ///    weight-row sums of classes 0..9.
    /// 8. `1000:0ff3`..`1000:101d` -- `dmg_min = strength div 2`,
    ///    `dmg_max = strength`, `hpmax = vitality * 5 + strength + 10`,
    ///    `hp = hpmax`.
    /// 9. `1000:102a`..`1000:114f` -- the two loot words, both built from the
    ///    same intermediate `k = крутизна div 2 + Round(class * крутизна / 5)`
    ///    (recomputed from scratch for each: `1000:1037`, `1000:106a`,
    ///    `1000:10cd` and `1000:110a` all `mov ax,[0x3952]` /
    ///    `mul word [0x395c]`). Хлам (`[0x396e]`) is
    ///    `Random(6) + 2 * Random(k) - k`, money (`[0x396c]`) is
    ///    `Random(6) + Random(k) - k div 2`, each floored at 0
    ///    (`1000:10b4`, `1000:1152`).
    /// 10. `1000:115e`..`1000:1181` -- beer, `Random(2) + крутизна div 10 + 1`.
    /// 11. `1000:1184`..`1000:11b9` -- armour, `Random(b) + b` stored as a
    ///     byte, where `b = 2 * (district - 1)^2` (`dec ax` / `imul ax` /
    ///     `shl ax,1` twice / `idiv 2`). District 1 therefore always draws
    ///     `Random(0)`, which the original's `Random` returns 0 for -- the
    ///     draw still happens, which is why `1000:1197` has 13 stops and
    ///     not 3.
    fn roll_enemy(&mut self, param_1: u8) -> Fighter {
        let mut cls = i32::from(self.rng.below_at("1000:0d26", 0x33)) + 1;
        for i in 1..=10 {
            if cls - i < 0 {
                cls = 10 - i;
                break;
            }
            cls -= i;
        }
        cls += i32::from(self.rng.below_at("1000:0d70", u16::from(self.district)));
        if self.flag_3693 {
            cls += i32::from(self.rng.below_at("1000:0d91", 4));
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

        let district_bonus =
            4 * i32::from(self.rng.below_at("1000:0dcc", u16::from(self.district)));
        let spread = i32::from(self.rng.below_at("1000:0ddd", 5));
        // `1000:0df0` is the DIVISOR and `1000:0e04` the multiplier, not the
        // other way round: `1000:0dfd` pushes the first as a real which
        // `1000:0e1e` pops back into `cx:si:di`, the divisor operand of
        // `0f78:1117`, while the second stays in `cx:bx` for the `0f78:09d2`
        // multiply against the player's level at `1000:0e10`.
        let divisor = i32::from(self.rng.below_at("1000:0df0", 2)) + 1;
        let factor = i32::from(self.rng.below_at("1000:0e04", 2)) + 1;
        // `divisor` is 1 or 2 and the numerator is doubled first, so this is
        // the real quotient exactly, with no rounding of its own.
        let twice = i32::from(self.player.level) * factor * 2 / divisor + 2 * (spread - 2);
        let mut ponty = Self::round_half(twice) + district_bonus;
        if ponty < 0 {
            ponty = 0;
        }
        if self.flag_3693 {
            ponty = Self::round_half(ponty * 3);
        }

        let weights = progress::class_weights(class);
        let sum = weights.iter().sum::<u16>() & 0xff;
        let points = ((i32::from(sum) + 2 * ponty) & 0xff) as u16;
        let mut stats = [0u16; 4]; // strength, agility, vitality, luck
        for _ in 0..points {
            let roll = self.rng.below_at("1000:0efd", sum) + 1;
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

        // Not a truncating port of the original here: at file 0x2907..0x290e
        // (`1000:1037`), `mov ax,[0x3952]` / `mul word [0x395c]` leaves the
        // full 32-bit `class * ponty` product in `dx:ax`, but the very next
        // byte is `cwd` (0x290e), which overwrites `dx` with the sign
        // extension of `ax` alone -- the high word of the product is
        // discarded before the divide-by-5-and-round that follows. This
        // port computes `2 * class * ponty` in `i32` and never truncates to
        // 16 bits, so it is wider than the original here. Unreachable at
        // realistic values (`class` caps at 9, `ponty` stays small), so
        // behaviour is unaffected in practice.
        let k = ponty / 2 + (2 * i32::from(class) * ponty + 5) / 10;
        let junk_bonus = i32::from(self.rng.below_at("1000:102e", 6));
        let junk_roll = i32::from(self.rng.below_at("1000:109c", k as u16));
        let junk = (junk_bonus + 2 * junk_roll - k).max(0);
        let money_bonus = i32::from(self.rng.below_at("1000:10c4", 6));
        let money_roll = i32::from(self.rng.below_at("1000:113c", k as u16));
        let money = (money_bonus + money_roll - k / 2).max(0);
        let beer_dl = i32::from(self.rng.below_at("1000:1162", 2)) + ponty / 10 + 1;
        let armour_base = 2 * (i32::from(self.district) - 1).pow(2);
        let armor =
            (i32::from(self.rng.below_at("1000:1197", armour_base as u16)) + armour_base) & 0xff;

        // `data/enemies.json` has one row per rolled class 0..=9 (classes
        // 0..9 are unique there; only the scripted class 10 has variants),
        // so this lookup is total for every class the clamps above can
        // leave. A nameless fighter must never reach the player, so a
        // missing row is a build-data bug, not something to paper over
        // with "".
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
            armor: armor as u16,
            dmg_min: strength / 2,
            dmg_max: strength,
            beer_dl: beer_dl as u16,
            money,
            junk: junk as u16,
            ..Fighter::default()
        }
    }

    /// `name`, the handler `1000:ecf1`'s compare dispatches. **Established
    /// from flow**, `1000:ecfb`..`1000:ed9c`:
    ///
    /// * `1000:ecfb`..`1000:ed24` -- `^2Звали тебя:^7 ` (file `0xC381`,
    ///   loaded at `1000:ed01` as image `0xaab1`) is assigned into a stack
    ///   temp with `0f78:0ae7`, the name variable `DS:379c` is appended with
    ///   `0f78:0b66`, and the result goes out through `0eed:01c2`
    ///   (`WriteLn`).
    /// * `1000:ed29`..`1000:ed3d` -- `^2А теперь будут:^7 ` (file `0xC392`,
    ///   image `0xaac2`) through `0eed:0000` (`Write`, no newline).
    /// * `1000:ed42`..`1000:ed5a` -- `ReadLn(Input, DS:379c)`.
    /// * `1000:ed5f` `cmp byte [0x379c],0` / `jnz 0xed79` -- an **empty**
    ///   line leaves the length byte at zero, and `1000:ed74` then assigns
    ///   `Раз^6дол^4бай` (file `0xC3A7`, image `0xaad7`) over the name with
    ///   `0f78:0b01`. Reproduced below; it is the same substitution
    ///   `src/main.rs`'s `create_character` already models for character
    ///   creation at `1000:7220`/`1000:7227` (file `0x80B4`). The test is on
    ///   the shortstring's **length byte**, not on whether the content is
    ///   all whitespace -- a line of only spaces has nonzero length and is
    ///   kept, not substituted. `Game::rename` must not `.trim()` the line
    ///   before this check.
    ///
    /// Both prompts are the game's own strings, not this port's wording. An
    /// earlier revision of this comment claimed they were invented and called
    /// itself "the one place the module knowingly departs from the
    /// byte-verbatim rule". There is no such place; see `docs/re/gaps.md`,
    /// "`rename`'s prompts -- the retraction was wrong; there is no
    /// deviation".
    ///
    /// **Not reproduced:** `1000:ed79`..`1000:ed9c` then rebuilds the stored
    /// name as `^7 ` + name (file `0xC3B5`, loaded at `1000:ed7f`) with the
    /// same three calls character creation makes at `1000:7245` / `1000:724f`
    /// / `1000:725d` (file `0x80C2`) -- which is why every
    /// save in `orig/` holds a name beginning `^7 `. This port stores the
    /// bare name in both paths; registered in the same `docs/re/gaps.md`
    /// entry.
    fn rename(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        term::print("^2Звали тебя:^7 ");
        term::println(&self.player.name);
        term::print("^2А теперь будут:^7 ");
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let n = line?;
        // 1000:ed5f `cmp byte [0x379c],0` tests the just-read shortstring's
        // LENGTH BYTE, not its trimmed content: a line of only spaces has a
        // nonzero length byte and is kept verbatim, only a genuinely empty
        // line (length byte zero) triggers `1000:ed74`'s substitution. Do
        // not `.trim()` `n` before this check -- that would substitute on
        // whitespace-only input, which the original does not do. `lines`
        // already strips the line terminator (`BufRead::lines`), so `n` here
        // is exactly the length-byte-tested string.
        self.player.name = if n.is_empty() {
            "Раз^6дол^4бай".to_string()
        } else {
            n
        };
        Ok(())
    }

    /// `kos`, the joint. The game has **two** copies of this handler and both
    /// are now reproduced -- see [`Joint`] for which is which and for the
    /// byte-level difference between them. This doc traces the top-level copy
    /// at `1000:e97d`..`1000:ea85` (the one `entry` dispatches):
    ///
    /// * broken jaw (`DS:38b0`) -> `^4Ты не схавать` ... (file `0xBEF3`).
    /// * already stoned (`DS:38cd != 0`) -> `^6Ты неможешь` ... (file `0xBFB8`).
    /// * no joints (`DS:38c5 <= 0`) -> `^4У тебя нет косяков` (file `0xBFA3`).
    /// * otherwise **exactly one** joint (`1000:e9b4`): stoned counter := 10
    ///   (`1000:e9b8` `c6 06 cd 38 0a`), strength += 2, `dmg_min` += 1,
    ///   `dmg_max` += 2, and heal a flat **+10** capped at `hpmax`, then
    ///   `^2Сила +2.` (file `0xBF98`).
    ///   The heal message splits like the beer routine's: when the shortfall
    ///   is under 10 it writes `^2Колёса прибавляют #з. ` (file `0xBF22`, no
    ///   newline) then `^2Здоровья:#/#. Осталось # косяков` (file `0xBF3B`);
    ///   otherwise the single combined line (`^2Колёса прибавляют` ..., file `0xBF5E`), whose "косякова" typo is the original's.
    ///
    /// `crate::model::Fighter` has a `stoned: bool`, not the original's
    /// countdown, so the flag is modelled as "stoned or not" and the
    /// countdown itself lives in [`Game::buff_countdown`].
    fn smoke(&mut self, site: Joint) {
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
        // 1000:e9b8 / 1000:4b52 set the countdown at 20ae:38cd -- to 10 at
        // the street prompt, to 3 inside a fight; the walk preamble decays it
        // (1000:aea8) and takes the buff back at zero. `Fighter::stoned` is
        // the same event as a bool, kept in step here.
        self.player.stoned = true;
        self.buff_countdown = site.buff_turns();
        self.player.strength += 2;
        self.player.dmg_min += 1;
        self.player.dmg_max += 2;
        let shortfall = self.player.hpmax.saturating_sub(self.player.hp);
        if shortfall < 10 {
            term::print(&text::fill("^2Колёса прибавляют #з. ", &[shortfall as i64]));
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
                site.long_heal_line(),
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

    /// `x` at the dealers: sell junk. `crate::model::Fighter::junk` exists
    /// and is rolled onto a defeated enemy (`roll_enemy`), but combat
    /// victory never awards it to the player -- `Game::run_combat` does not
    /// yet reproduce `1000:523e`..`1000:5251` (see `docs/re/gaps.md`). The
    /// player's `junk` therefore always stays at 0, so the "nothing to
    /// sell" branch is always true (`^4Тебе нечего спихнуть.`, file
    /// `0xAFC2`).
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
    ///
    /// **The debit is 3 as well -- established from flow**, not inferred:
    /// `1000:d5d9` is `83 2e c7 38 03`, `sub word [0x38c7],0x3`, reached
    /// through the submenu's `h` compare at `1000:d5b9` (token file
    /// `0xB392`). An earlier revision of this comment called it an inference
    /// because "the vet's own submenu handler was not traced"; the `difftest`
    /// task traced it and this comment did not follow. See
    /// `docs/re/gaps.md`, "The vet's charged amounts".
    fn heal_jaw(&mut self) {
        self.pay_and_heal(3, self.player.broken_jaw, |f| f.broken_jaw = false);
    }

    /// `r` at the vet: 7 rubles to fix a broken leg (file `0xB2D9`,
    /// `cmp word [0x38c7],0x7` at `1000:d465`). Its debit is
    /// `1000:d553` `83 2e c7 38 07`, reached through the `r` compare at
    /// `1000:d537` (token file `0xB320`) -- **established from flow**, same
    /// as [`Game::heal_jaw`]. Note the two arms sit in the opposite order to
    /// the two menu rows, which is why they are paired by key, not position.
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
    /// Only `Market`/`Dealers` have a row table (`data/shops.json` covers
    /// just `mar`/`bmar`).
    ///
    /// ## At the dealers the district gate is a MENU gate, not a buy gate
    ///
    /// **Established from flow** (`docs/re/shop-arms.md`, Task 23). The
    /// `bmar` handler holds five `cmp byte [0x3692]` tests -- `1000:c68d`
    /// (row 5, `jbe 0xc6f1`), `1000:c6f1` (row 6, `jbe 0xc755`),
    /// `1000:c755` (row 7, `jbe 0xc7ba`), `1000:c7ba` (row 8,
    /// `jbe 0xc81d`) and `1000:c81d` (row 9, `jbe 0xc88e`) -- and every one
    /// of them sits in the menu-print block, deciding which lines are
    /// LISTED. The arms reached when the player types a key carry no
    /// district test at all: an aligned decode of
    /// `1000:c8ce`..`1000:ccc4` (rows 1-6) and `1000:ccc4`..`1000:ce80`
    /// (rows 7-9) finds no operand equal to `0x3692`, and the byte pair
    /// `92 36` does not occur in either span, so there is not even a
    /// byte-scan candidate to discard. Typing `5` at district 1 buys the
    /// Кастет off a menu that never listed it.
    ///
    /// So [`Game::print_priced_rows`] keeps its gate and the buy path below
    /// drops it -- **for `bmar` only**.
    ///
    /// ## At the market it is BOTH, for three rows out of four
    ///
    /// **Established from flow** (`docs/re/shop-arms.md`'s `mar` half and
    /// `data/shop_arms.json`'s `mar` key, Task 25, re-derived from
    /// `orig/g.exe` by `python3 tools/test_shop_arms.py`). Symmetry with
    /// `bmar` would have been the wrong answer here, which is why the two
    /// shops were measured over their own ranges: `mar`'s buy path DOES read
    /// `20ae:3692`, at three sites. `1000:c08e cmp byte [0x3692],0x1`
    /// (`1000:c093 ja 0xc098`), `1000:c1d7 cmp byte [0x3692],0x2`
    /// (`1000:c1dc ja 0xc1e1`) and `1000:c27f cmp byte [0x3692],0x3`
    /// (`1000:c284 ja 0xc289`) stand in front of rows 6, 8 and 9, so those
    /// three really are unbuyable below their district. Each prints
    /// **nothing**: the gate sits ahead of the row's key compare, so
    /// `1000:c095`, `1000:c1de` and `1000:c286` jump straight to the row's
    /// span end and the line falls through to the handler's own re-prompt at
    /// `1000:c47b` exactly as an unrecognised key does.
    ///
    /// **Row 7 is menu-gated and NOT buy-gated -- a divergence reproduced,
    /// not fixed.** The menu gate `1000:bb80` covers two lines, rows 6 and 7
    /// (`20ae:0b33` loaded at `1000:bb8a` and `20ae:0b34` at `1000:bbe6`,
    /// both inside `1000:bb8a`..`1000:bc42`), while the buy path gates only
    /// row 6: `1000:c095 jmp 0xc142` lands on row 7's `mov di,0x3a72` setup
    /// itself, with nothing between it and the key compare at `1000:c14c`.
    /// So at district 1 the market lists rows 1-5, typing `6` prints not a
    /// word, and typing `7` buys the adidas suit for 30 руб. and applies its
    /// armour. `data/shops.json` keeps `district>1` on row 7 because that is
    /// the MENU gate, and [`Game::buy_market_row`] deliberately does not
    /// consult `row.gate` at all -- it carries the three buy-path
    /// immediates itself.
    ///
    /// Which is why [`Game::gate_open`] has exactly **one** caller now, the
    /// menu filter [`Game::listed_rows`]:
    /// `grep -c 'self[.]gate_open(' src/game.rs` returns **1**, against **2**
    /// for the same pattern at `fef8c9c`. **Count calls, not mentions.** The
    /// looser `grep -c 'self.gate_open'` returns 2 at both revisions --
    /// this very sentence moved into the deleted call's place -- and
    /// `data/shop_arms.json` shipped that unfalsifiable form until the final
    /// whole-branch review caught it: a count invariant across the change it
    /// existed to witness. No buy path consults `row.gate` any more, in
    /// either shop -- for opposite reasons.
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
        // Every row of both shops has an arm of its own -- its own gates, its
        // own refusal lines, its own confirmation and its own effect. See
        // [`Game::buy_dealer_row`] and [`Game::buy_market_row`].
        //
        // There is no generic "debit the price and echo the menu line" path
        // left: the original has none, that echo was this port's invention,
        // and both functions cover all nine keys of their shop. The two
        // `#[test]`s `every_dealers_row_has_an_arm_of_its_own` and
        // `every_market_row_has_an_arm_of_its_own` are what keep that true if
        // `data/shops.json` ever grows a row.
        let handled = match tag {
            "mar" => self.buy_market_row(row.key, row.price),
            _ => self.buy_dealer_row(row.key, row.price),
        };
        debug_assert!(handled, "{tag} row {} has no purchase arm", row.key);
    }

    /// The shape all **eighteen** purchase arms share -- `bmar` rows 1..9
    /// ([`Game::buy_dealer_row`]) and `mar` rows 1..9
    /// ([`Game::buy_market_row`]) -- applied in the order the original tests
    /// it: the prerequisite / better-item gate where the row has one, then
    /// the already-own gate, then affordability, then the debit, then the
    /// effect.
    ///
    /// `gates` is `(refuse, line)` in image order. `line` is `None` for a
    /// gate that prints nothing at all. There are five such gates across the
    /// two shops: `bmar` row 9's first two, whose branches `1000:cdfe` and
    /// `1000:ce05` both land on `1000:ce76`, the six-instruction SETUP for
    /// the next compare (`1000:ce80`, the `x` verb) rather than the compare
    /// itself; and `mar`'s three district gates `1000:c08e`, `1000:c1d7` and
    /// `1000:c27f`, whose skips `1000:c095`, `1000:c1de` and `1000:c286` land
    /// on the next row's span start with nothing printed.
    ///
    /// The `mar` district gates are the one place a gate here does NOT sit
    /// where the original's does: all three stand in front of their row's key
    /// compare, not behind it. The behaviour is the same either way, because
    /// no compare the skip can reach matches the skipped row's digit again:
    ///
    /// * `1000:c095` lands on row 7's setup `1000:c142`, whose compare
    ///   `1000:c14c` tests `7`.
    /// * `1000:c1de` lands on row 9's own **district gate** `1000:c27f
    ///   cmp byte [0x3692],0x3` -- not its setup `1000:c289` and not its
    ///   compare -- whose two exits are `1000:c289` (reaching the compare
    ///   against `9` at `1000:c293`) and `1000:c31f`. Neither compares `8`.
    /// * `1000:c286` lands on `1000:c31f`, the setup for the pickpocket verb
    ///   `t` at `1000:c329`.
    ///
    /// So the typed line reaches the re-prompt at `1000:c47b` in silence,
    /// which is what a leading silent gate produces here. An earlier revision
    /// of this comment said `1000:c1de` lands on "row 9's compare against
    /// `9` (`1000:c293`)"; the conclusion was right and the instruction named
    /// was not.
    ///
    /// The money test is last in every one of the eighteen and is the same
    /// three instructions each time (`mov al,[price]` / `xor ah,ah` /
    /// `cmp ax,[0x38c7]`) followed by a `jle` to the buy. At `bmar`:
    /// `1000:c8e8`, `1000:c94c`, `1000:c9c8`, `1000:cae8`, `1000:cb80`,
    /// `1000:cc39`, `1000:cce8`, `1000:cd86` and `1000:ce17`. At `mar`:
    /// `1000:bd91`, `1000:be27`, `1000:bed9`, `1000:bf63`, `1000:c00c`,
    /// `1000:c0c3`, `1000:c166`, `1000:c205` and `1000:c2ad`. Every one of
    /// those eighteen branch bytes is `7e`, checked per row rather than
    /// assumed, so the sale goes through when `price <= money` and the
    /// *refusal* is the fall-through; only the wording differs between rows.
    ///
    /// The debit itself is `sub [0x38c7],ax` at `1000:c90a`, `1000:c973`,
    /// `1000:c9eb`, `1000:cb0f`, `1000:cba7`, `1000:cc60`, `1000:cd14`,
    /// `1000:cdad` and `1000:ce3e` (`bmar`), and at `1000:bdb3`,
    /// `1000:be49`, `1000:bf00`, `1000:bf8a`, `1000:c033`, `1000:c0ea`,
    /// `1000:c18d`, `1000:c22c` and `1000:c2d4` (`mar`).
    ///
    /// **The write-before-debit order is not observable, in either shop.**
    /// At `mar` rows 3-9 all set their ownership flag ahead of the debit
    /// (`1000:bef6`, `1000:bf80`, `1000:c029`, `1000:c0e0`, `1000:c183`,
    /// `1000:c222`, `1000:c2ca`) and rows 1 and 2 write no flag at all; none
    /// of the seven reads the money or the byte it just wrote between the
    /// write and the `sub`, and the three upgrade guards read a *different*
    /// flag from the one their arm sets (`1000:c1aa` reads `20ae:38b4` while
    /// `1000:c183` wrote `20ae:38b7`; `1000:c249` reads `20ae:38b5` while
    /// `1000:c222` wrote `20ae:38b8`; `1000:c2f1` reads `20ae:38b6` while
    /// `1000:c2ca` wrote `20ae:38b9`). So the effect closure runs after the
    /// debit for all eighteen.
    ///
    /// At `bmar`, **only rows 1 and 3 debit before they write anything
    /// else.** The other seven write first, and this comment said "three
    /// arms" until a review
    /// recounted them -- the inventory-that-stopped-early defect
    /// `docs/re/METHODOLOGY.md` names, sitting inside the justification for a
    /// deliberate divergence. Six of the seven set an ownership FLAG ahead of
    /// the debit:
    ///
    /// | row | write | debit |
    /// |---|---|---|
    /// | 2 | `1000:c969` `mov byte [0x38bb],0x1` | `1000:c973` |
    /// | 4 | `1000:cb05` `mov byte [0x38bc],0x1` | `1000:cb0f` |
    /// | 5 | `1000:cb9d` `mov byte [0x38ba],0x1` | `1000:cba7` |
    /// | 6 | `1000:cc56` `mov byte [0x394b],0x1` | `1000:cc60` |
    /// | 7 | `1000:cd05` `mov byte [0x394d],0x1` | `1000:cd14` |
    /// | 9 | `1000:ce34` `mov byte [0x394e],0x1` | `1000:ce3e` |
    ///
    /// The seventh is row 8's `1000:cda3 add word [0x394f],0x5`, a COUNT
    /// rather than a flag, ahead of `1000:cdad`; row 7 adds to that same
    /// count at `1000:cd0a`, also before its debit. In every one of the seven
    /// the instructions between the write and the `sub` read neither the
    /// money nor the thing written, so the order is not observable and the
    /// effect closure runs after the debit here.
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
        if self.player.money < price {
            term::println(too_poor);
            return;
        }
        self.player.money -= price;
        effect(self);
    }

    /// The dealers' nine purchase arms -- `bmar` rows 1..9. Returns `true`
    /// when the key was one of the nine and the arm has run. The caller has
    /// no fall-through left for a `false` to reach: Task 26 deleted the
    /// generic "debit and echo the menu line" path, so a `false` here means a
    /// row of `data/shops.json` with no arm, which
    /// [`Game::shop_action`]'s `debug_assert!` catches in debug and
    /// `every_dealers_row_has_an_arm_of_its_own` catches in either profile.
    ///
    /// **Established from flow.** Rows 7-9 are Task 18's; rows 1-6 are
    /// Task 24's, off the map `docs/re/shop-arms.md` / `data/shop_arms.json`
    /// (which `python3 tools/test_shop_arms.py` re-derives from
    /// `orig/g.exe`). One line is read by the `ReadLn` at `1000:c8c9` after
    /// the prompt `^0Барыги\` (CS `0x937b`), and each row then compares that
    /// one buffer at `20ae:3a72` against its own one-character literal with
    /// `0f78:0bd8`. Each miss branch targets the *next* row's setup and each
    /// arm's tail rejoins there, so the nine are a chain of independent
    /// `if`s over one buffer, not an `if`/`else` -- the same shape
    /// `docs/re/combat-dispatch.md` records for the combat prompt.
    ///
    /// | row | key compare | key literal | price | debit |
    /// |---|---|---|---|---|
    /// | 1 Косяк | `1000:c8d8` | CS `0x8dca` | `20ae:0b38` = 15 | `1000:c90a` |
    /// | 2 Краденый мобильник | `1000:c935` | CS `0x8e4b` | `20ae:0b39` = 30 | `1000:c973` |
    /// | 3 Офигенный косяк | `1000:c9b5` | CS `0x8ea5` | `20ae:0b3a` = 20 | `1000:c9eb` |
    /// | 4 зоновская наколка | `1000:cad1` | CS `0x8ef7` | `20ae:0b3b` = 10 | `1000:cb0f` |
    /// | 5 Кастет | `1000:cb51` | CS `0x8f6b` | `20ae:0b3c` = 25 | `1000:cba7` |
    /// | 6 Дубинка | `1000:cc0e` | CS `0x8fc6` | `20ae:0b3d` = 50 | `1000:cc60` |
    /// | 7 пистолет | `1000:ccce` | CS `0x9023` | `20ae:0b3e` = 150 | `1000:cd14` |
    /// | 8 патроны | `1000:cd6f` | CS `0x9055` | `20ae:0b3f` = 70 | `1000:cdad` |
    /// | 9 глушитель | `1000:cdef` | CS `0x906a` | `20ae:0b40` = 60 | `1000:ce3e` |
    ///
    /// **A label correction this carries.** Earlier revisions of this comment
    /// and of [`crate::combat_dispatch::Pistol`] called `1000:ccd8`,
    /// `1000:cd76` and `1000:cdf9` the three key compares. They are not: each
    /// decodes to `cmp byte [0x394d],0x0`, the arm's own pistol gate. The key
    /// compares are `1000:ccce`, `1000:cd6f` and `1000:cdef` (each
    /// `call 0xf78:0xbd8`), and the addresses in the table above are the ones
    /// `python3 tools/re_query.py resolve <citation>` decodes. `docs/re/gaps.md`
    /// records the correction.
    ///
    /// **No arm of the nine tests the district** -- see [`Game::shop_action`].
    /// Rows 1 and 3 have no already-own test and are **repeatable**; rows 2,
    /// 4, 5, 6, 7 and 9 are one-shot through their own already-own test, and
    /// row 8 through none at all.
    ///
    /// Row 3 is the only one that draws (`Random(4)` at `1000:ca0c`), so a
    /// purchase there advances the RNG stream.
    fn buy_dealer_row(&mut self, key: &str, price: i32) -> bool {
        match key {
            // Row 1, Косяк. Key compare `1000:c8d8`, miss
            // `1000:c8dd jnz 0xc92b`. One gate only -- no already-own test
            // and no prerequisite, so the row is repeatable.
            "1" => {
                self.buy_after_gates(
                    price, // 20ae:0b38 = 15
                    &[],
                    // CS 0x9385 `^4Чёрт, бабок не хватает.`, pushed at 1000:c8ea. This literal is row 1's
                    // own; the port used to print it for every dealers' row.
                    "^4Чёрт, бабок не хватает.",
                    |g| {
                        // 1000:c90a `sub [0x38c7],ax`, then 1000:c90e
                        // `inc [0x38c5]` -- a word COUNT of joints, not a
                        // flag. Read by the sheet at 1000:23b4 and by `kos`
                        // at 1000:4b44 (in a fight) and 1000:e9aa (at the
                        // street prompt), so the effect is fully consumed.
                        g.player.joints += 1;
                        term::println("^2Ты купил косяк"); // CS 0x939f `^2Ты купил косяк`, 1000:c912
                    },
                );
                true
            }
            // Row 2, Краденый мобильник. Key compare `1000:c935`, miss
            // `1000:c93a jnz 0xc9ab`.
            "2" => {
                // 1000:c93c `cmp byte [0x38bb],0x0` / 1000:c941 `jnz 0xc992`.
                let owned = self.has_mobile;
                self.buy_after_gates(
                    price, // 20ae:0b39 = 30
                    // CS 0x93d6 `^6У тебя уже есть мобила.`, pushed at 1000:c992.
                    &[(owned, Some("^6У тебя уже есть мобила."))],
                    "^4Нету денег", // CS 0x93b0 `^4Нету денег`, 1000:c94e
                    |g| {
                        // 1000:c969 `mov byte [0x38bb],0x1`; debit 1000:c973.
                        // Read by the sheet at 1000:1cd8, by the in-combat
                        // backup countdown at 1000:4cdb -- which is what the
                        // menu line's "подмога быстрее приходит" actually is
                        // -- and by five wander sites (1000:af3d, 1000:af7d,
                        // 1000:afe3, 1000:b022, 1000:b0ce).
                        g.has_mobile = true;
                        term::println("^2Чё ты модный типа да?."); // CS 0x93bd `^2Чё ты модный типа да?.`, 1000:c977
                    },
                );
                true
            }
            // Row 3, Офигенный косяк. Key compare `1000:c9b5`; the miss is an
            // inverted pair, `1000:c9ba jz 0xc9bf` over `1000:c9bc jmp
            // 0xcac7`, because the arm is too long for a short branch. One
            // gate, so the row is repeatable and each purchase rolls again.
            "3" => {
                self.buy_after_gates(
                    price, // 20ae:0b3a = 20
                    &[],
                    "^4Не хватает", // CS 0x8e4d `^4Не хватает`, 1000:c9ca
                    |g| {
                        // Debit 1000:c9eb, then the line, then the draw.
                        term::println("^2Пошли стероиды!"); // CS 0x93f0 `^2Пошли стероиды!`, 1000:c9ef

                        // 1000:ca0c `call 0f78:114b` with `mov ax,0x4` at
                        // 1000:ca08, dispatched over four compares at
                        // 1000:ca11, 1000:ca53, 1000:ca77 and 1000:caa5.
                        match g.rng.below_at("1000:ca0c", 4) {
                            0 => {
                                g.player.strength += 1; // 1000:ca16 inc [0x389e]
                                term::println("^1Сила +1 "); // CS 0x9402 `^1Сила +1 `, 1000:ca1a
                                g.player.dmg_max += 1; // 1000:ca33 inc [0x38aa]

                                // 1000:ca37..1000:ca43 -- `mov ax,[0x389e]` /
                                // `cwd` / `mov cx,0x2` / `idiv cx` /
                                // `xchg ax,dx` / `or ax,ax` /
                                // `jnz 0xca49`, so the dmg-min half runs only
                                // when the NEW Сила is even. It is the mirror
                                // of the in-combat stat-loss arm at
                                // 1000:498f, which takes its dmg-min half
                                // when Сила is odd.
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
            // Row 4, зоновская наколка. Key compare `1000:cad1`, miss
            // `1000:cad6 jnz 0xcb47`.
            "4" => {
                // 1000:cad8 `cmp byte [0x38bc],0x0` / 1000:cadd `jnz 0xcb2e`.
                let owned = self.prison_tattoo;
                self.buy_after_gates(
                    price, // 20ae:0b3b = 10
                    // CS 0x9446 `^6Сделать, конечно, можно но толку не будет.`, pushed at 1000:cb2e.
                    &[(owned, Some("^6Сделать, конечно, можно но толку не будет."))],
                    "^4Нету денег", // CS 0x93b0 `^4Нету денег`, 1000:caea -- row 2's literal
                    |g| {
                        // 1000:cb05 `mov byte [0x38bc],0x1`; debit 1000:cb0f.
                        // The flag has four references image-wide, two
                        // outside this arm: the sheet at 1000:1d18 and
                        // 1000:b5da, the wander mugging roll, which halves
                        // the chance when it is set. That single branch is
                        // the row's entire gameplay effect.
                        g.prison_tattoo = true;
                        term::println("^2Чистый зек."); // CS 0x9438 `^2Чистый зек.`, 1000:cb13
                    },
                );
                true
            }
            // Row 5, Кастет. Key compare `1000:cb51`; the miss is the
            // inverted pair `1000:cb56 jz 0xcb5b` over `1000:cb58 jmp
            // 0xcc04`.
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
                let better =
                    self.weapon_dubinka_394b && self.weapon_nozhik_38c2 && self.weapon_tesak_394c;
                // 1000:cb70 `cmp byte [0x38ba],0x0` / 1000:cb75 `jnz 0xcbd0`.
                let owned = self.weapon_kastet_38ba;
                self.buy_after_gates(
                    price, // 20ae:0b3c = 25
                    &[
                        // CS 0x94da `^6Нафиг тебе он нужен, когда есть более мощное оружие.`, pushed at 1000:cbeb.
                        (
                            better,
                            Some("^6Нафиг тебе он нужен, когда есть более мощное оружие."),
                        ),
                        // CS 0x94bf `^6У тебя есть эта железка.`, pushed at 1000:cbd0.
                        (owned, Some("^6У тебя есть эта железка.")),
                    ],
                    "^4Не хватает деньжат", // CS 0x9473 `^4Не хватает деньжат`, 1000:cb82
                    |g| {
                        g.weapon_kastet_38ba = true; // 1000:cb9d mov byte [0x38ba],0x1

                        // Debit 1000:cba7. The +2/+2 is unconditional here.
                        g.player.dmg_min += 2; // 1000:cbab add word [0x38a8],0x2
                        g.player.dmg_max += 2; // 1000:cbb0 add word [0x38aa],0x2

                        // CS 0x9488 `^2Ты купил кастет смотри чтоб менты с ним не запалили.`, pushed at 1000:cbb5.
                        term::println("^2Ты купил кастет смотри чтоб менты с ним не запалили.");
                    },
                );
                true
            }
            // Row 6, Дубинка. Key compare `1000:cc0e`; miss
            // `1000:cc13 jz 0xcc18` over `1000:cc15 jmp 0xccc4`.
            "6" => {
                // Two conjuncts this time -- 1000:cc18 `cmp byte [0x38c2],0x0`
                // / 1000:cc1d `jz 0xcc29` and 1000:cc1f
                // `cmp byte [0x394c],0x0` / 1000:cc24 `jz 0xcc29`, falling to
                // 1000:cc26 `jmp 0xccab` only when
                // both are set. Same AND/OR mismatch with the loot arm
                // (1000:55c5, 1000:55cc) as row 5.
                let better = self.weapon_nozhik_38c2 && self.weapon_tesak_394c;
                // 1000:cc29 `cmp byte [0x394b],0x0` / 1000:cc2e `jnz 0xcc90`.
                let owned = self.weapon_dubinka_394b;
                // 1000:cc64 `cmp byte [0x38ba],0x0` / 1000:cc69 `jz 0xcc75`.
                let kastet = self.weapon_kastet_38ba;
                self.buy_after_gates(
                    price, // 20ae:0b3d = 50
                    &[
                        // CS 0x957c `^6Да нафиг она нужна, когда есть более мощное оружие.`, pushed at 1000:ccab.
                        (
                            better,
                            Some("^6Да нафиг она нужна, когда есть более мощное оружие."),
                        ),
                        // CS 0x9566 `^6У тебя есть дубина.`, pushed at 1000:cc90.
                        (owned, Some("^6У тебя есть дубина.")),
                    ],
                    "^4Не хватает на дубинку деньжат", // CS 0x9511 `^4Не хватает на дубинку деньжат`, 1000:cc3b
                    |g| {
                        g.weapon_dubinka_394b = true; // 1000:cc56 mov byte [0x394b],0x1

                        // Debit 1000:cc60.
                        //
                        // ORIGINAL BUG, reproduced: the menu line advertises
                        // `урон+4`, and 1000:cc69 `jz 0xcc75` skips BOTH adds
                        // when the knuckles are not owned -- its target is the
                        // confirmation push, and there is no other add on that
                        // path. So buying the club first costs 50 руб., sets
                        // the flag, prints the confirmation and changes the
                        // damage range by nothing. The loot arm granting the
                        // same club tests the same flag and has both halves:
                        // 1000:55d3 / `jz 0x55e6`, +2/+2 at 1000:55da and
                        // 1000:55df, +4/+4 at 1000:55e6 and 1000:55eb. The
                        // shop arm is the loot arm with the `+4` branch
                        // missing.
                        if kastet {
                            g.player.dmg_min += 2; // 1000:cc6b add word [0x38a8],0x2
                            g.player.dmg_max += 2; // 1000:cc70 add word [0x38aa],0x2
                        }
                        // CS 0x9531 `^2Ты купил дубинку - похоже задумал чё-то нехорошее.`, pushed at 1000:cc75 -- exactly where
                        // 1000:cc69 jumps.
                        term::println("^2Ты купил дубинку - похоже задумал чё-то нехорошее.");
                    },
                );
                true
            }
            // Row 7, самопальный пистолет. Key compare `1000:ccce`, with
            // `1000:ccd3 jz 0xccd8` in front of it.
            "7" => {
                // 1000:ccd8 `cmp byte [0x394d],0x0` / 1000:ccdd `jnz 0xcd4c`.
                let owned = self.pistol.owned;
                self.buy_after_gates(
                    price, // 20ae:0b3e = 150
                    // CS 0x961e `^6Ну.. ты.. ВАЩЕ ОФИГЕЛ!`, pushed at 1000:cd4c.
                    &[(owned, Some("^6Ну.. ты.. ВАЩЕ ОФИГЕЛ!"))],
                    "^4Дорогая штука!", // CS 0x95b2 `^4Дорогая штука!`, 1000:ccea
                    |g| {
                        g.pistol.owned = true; // 1000:cd05 mov byte [0x394d],0x1
                        g.pistol.cartridges += 3; // 1000:cd0a add word [0x394f],0x3

                        // CS 0x95c3 `^2Спасайся кто может!!!`, pushed at 1000:cd18.
                        term::println("^2Спасайся кто может!!!");
                        term::println(
                            // CS 0x95db `^0Только помни стреляй в бандитских районах - там менты не накроют`, pushed at 1000:cd31.
                            "^0Только помни стреляй в бандитских районах - там менты не накроют",
                        );
                    },
                );
                true
            }
            // Row 8, патроны. Key compare `1000:cd6f`, miss
            // `1000:cd74 jnz 0xcde5`.
            "8" => {
                // 1000:cd76 `cmp byte [0x394d],0x0` / 1000:cd7b `jz 0xcdcc`.
                let no_gun = !self.pistol.owned;
                self.buy_after_gates(
                    price, // 20ae:0b3f = 70
                    // CS 0x9666 `^6Нету пушки. Сначала купи пистолет`, pushed at 1000:cdcc.
                    &[(no_gun, Some("^6Нету пушки. Сначала купи пистолет"))],
                    "^4Нехватка денег.", // CS 0x9637 `^4Нехватка денег.`, 1000:cd88
                    |g| {
                        // 1000:cda3 adds FIVE, though the menu line says six.
                        g.pistol.cartridges += 5;
                        // CS 0x9649 `^2Получи пять пуль.. на руки`, pushed at 1000:cdb1.
                        term::println("^2Получи пять пуль.. на руки");
                    },
                );
                true
            }
            // Row 9, глушитель. Key compare `1000:cdef`, with
            // `1000:cdf4 jz 0xcdf9` in front of it.
            "9" => {
                // 1000:cdf9 `cmp byte [0x394d],0x0` / 1000:cdfe `jz 0xce76`,
                // and 1000:ce00 `cmp byte [0x3e32],0x19` / 1000:ce05
                // `jnz 0xce76`. Both land on 1000:ce76, the setup for the
                // `x` compare at 1000:ce80, and print nothing at all -- the
                // only silent gates among the nine.
                let no_gun = !self.pistol.owned;
                let not_delivered = self.dealer_delivery_counter != 25;
                // 1000:ce07 `cmp byte [0x394e],0x0` / 1000:ce0c `jnz 0xce5d`.
                let owned = self.pistol.silencer;
                // Row 9 is the only reader of `20ae:3e32` besides the walk
                // counter that feeds it, so the dealers' 25-walk delivery is
                // the silencer's and nothing else's.
                self.buy_after_gates(
                    // 20ae:0b40 = 60, though the menu line prints 70 --
                    // `docs/re/tables.md` §2's split, reproduced.
                    price,
                    &[
                        (no_gun, None),
                        (not_delivered, None),
                        // CS 0x96b8 `^6Да купил уже, купил`, pushed at 1000:ce5d.
                        (owned, Some("^6Да купил уже, купил")),
                    ],
                    "^4Подкопи бабла.", // CS 0x968a `^4Подкопи бабла.`, 1000:ce19
                    |g| {
                        g.pistol.silencer = true; // 1000:ce34 mov byte [0x394e],0x1

                        // CS 0x969b `^2Теперь стреляй где хочешь!`, pushed at 1000:ce42.
                        term::println("^2Теперь стреляй где хочешь!");
                    },
                );
                true
            }
            _ => false,
        }
    }

    /// The market's nine purchase arms -- `mar` rows 1..9. Returns `true`
    /// when the key was one of the nine and the arm has run.
    ///
    /// **Established from flow.** The map is `docs/re/shop-arms.md`'s `mar`
    /// half and `data/shop_arms.json`'s `mar` key (Task 25), both re-derived
    /// from `orig/g.exe` by `python3 tools/test_shop_arms.py`; the `src/`
    /// half is Task 26's. One line is read by the `ReadLn` at `1000:bd43`
    /// after the prompt `^0Базар\` (CS `0x8dc1`, pushed at `1000:bd08`), and
    /// each row then compares that one buffer at `20ae:3a72` against its own
    /// one-character literal with `0f78:0bd8`. Each miss branch targets the
    /// next row's span start and each arm's tail rejoins there, so the nine
    /// are a chain of independent `if`s over one buffer. Row 9's miss lands
    /// on `1000:c31f`, whose compare at `1000:c329` is the market pickpocket
    /// verb `t` -- which is what bounds the nine on the right.
    ///
    /// | row | key compare | key literal | price | debit |
    /// |---|---|---|---|---|
    /// | 1 Хотдог | `1000:bd52` | CS `0x8dca` | `20ae:0b2e` = 2 | `1000:bdb3` |
    /// | 2 Пиво | `1000:be14` | CS `0x8e4b` | `20ae:0b2f` = 5 | `1000:be49` |
    /// | 3 Затемнённые очки | `1000:bec2` | CS `0x8ea5` | `20ae:0b30` = 10 | `1000:bf00` |
    /// | 4 abibas | `1000:bf42` | CS `0x8ef7` | `20ae:0b31` = 15 | `1000:bf8a` |
    /// | 5 Понтовые бутсы | `1000:bfeb` | CS `0x8f6b` | `20ae:0b32` = 15 | `1000:c033` |
    /// | 6 Реальную кожанку | `1000:c0a2` | CS `0x8fc6` | `20ae:0b33` = 25 | `1000:c0ea` |
    /// | 7 adidas | `1000:c14c` | CS `0x9023` | `20ae:0b34` = 30 | `1000:c18d` |
    /// | 8 Понтовёйшие бутсы | `1000:c1eb` | CS `0x9055` | `20ae:0b35` = 30 | `1000:c22c` |
    /// | 9 Ваще крутую кожанку | `1000:c293` | CS `0x906a` | `20ae:0b36` = 50 | `1000:c2d4` |
    ///
    /// **Three arms test the district and one that should does not** -- see
    /// [`Game::shop_action`] for the measurement and for why row 7 is sold at
    /// district 1 off a menu that never listed it.
    ///
    /// Rows 1 and 2 have no already-own test and are **repeatable**; rows 3-9
    /// are one-shot through their own.
    ///
    /// **Two arms draw.** Row 1's `Random(2)` at `1000:bdbb` is consumed
    /// arithmetically (`1000:bdc0 add ax,0x3`); row 2's `Random(3)` at
    /// `1000:be51` picks one of three confirmation lines and **changes no
    /// state at all**. Skipping the second because nothing depends on its
    /// result would desynchronise every draw after the first beer, so it is
    /// drawn here too.
    ///
    /// **Rows 7, 8 and 9 grant the upgrade DELTA, not the advertised bonus**
    /// (`1000:c1af`, `1000:c24e`, `1000:c2f6`). Applying the full bonus
    /// unconditionally would double-count whenever the lesser item is already
    /// owned; the totals come out the same in either purchase order, which is
    /// what the gym's recompute at `1000:e3a4`..`1000:e3e2` subtracts back
    /// out (`docs/re/gaps.md`; corroboration, not part of this decode).
    fn buy_market_row(&mut self, key: &str, price: i32) -> bool {
        match key {
            // Row 1, Хотдог. Setup 1000:bd48, key compare 1000:bd52; the miss
            // is an inverted pair, 1000:bd57 `jz 0xbd5c` over 1000:bd59
            // `jmp 0xbe0a`. Three gates, no already-own test, repeatable.
            "1" => {
                // 1000:bd5c `cmp byte [0x38b0],0x1` / 1000:bd61 `jnz 0xbd7f`.
                // The branch jumps PAST the refusal, so this gate refuses on
                // the FALL-THROUGH -- the opposite sense to every other
                // already-own/prerequisite gate in either shop.
                let jaw = self.player.broken_jaw;
                // 1000:bd7f `mov ax,[0x38ac]` / 1000:bd82 `cmp ax,[0x38ae]` /
                // 1000:bd86 `jnl 0xbdf1` -- refuse when hp is already at max.
                let healthy = self.player.hp >= self.player.hpmax;
                self.buy_after_gates(
                    price, // 20ae:0b2e = 2
                    &[
                        // CS 0x8dcc `^4Ты не можешь хавать из-за сломаной челюсти.`, pushed at 1000:bd63.
                        (jaw, Some("^4Ты не можешь хавать из-за сломаной челюсти.")),
                        // CS 0x8e37 `^6Да неохота хавать`, pushed at 1000:bdf1.
                        (healthy, Some("^6Да неохота хавать")),
                    ],
                    // CS 0x8dfa `^4Чёрт, бабок даже на жратву не хватает.`, pushed at 1000:bd93. This is the literal the
                    // port's now-deleted generic path used to print for every
                    // `mar` row; it belongs to row 1 alone.
                    "^4Чёрт, бабок даже на жратву не хватает.",
                    |g| {
                        // Debit 1000:bdb3, then 1000:bdb7 `mov ax,0x2` /
                        // 1000:bdbb `call 0f78:114b` (the `n` recovered with
                        // `python3 tools/re_query.py pushed-n 1000:bdbb`),
                        // 1000:bdc0 `add ax,0x3`, 1000:bdc3
                        // `add [0x38ac],ax`. The hot dog heals 3 or 4, and
                        // nothing stands between the draw and the add.
                        g.player.hp += 3 + g.rng.below_at("1000:bdbb", 2);
                        // 1000:bdc7 `mov ax,[0x38ac]` / 1000:bdca
                        // `cmp ax,[0x38ae]` / 1000:bdce `jle 0xbdd6`, else
                        // 1000:bdd0 `mov ax,[0x38ae]` / 1000:bdd3
                        // `mov [0x38ac],ax` writes hp max back into hp.
                        if g.player.hp > g.player.hpmax {
                            g.player.hp = g.player.hpmax;
                        }
                        term::println("^2Ты сожрал хот-дог"); // CS 0x8e23 `^2Ты сожрал хот-дог`, 1000:bdd6
                    },
                );
                true
            }
            // Row 2, Пиво. Setup 1000:be0a, key compare 1000:be14, miss
            // 1000:be19 `jz 0xbe1e` over 1000:be1b `jmp 0xbeb8`. One gate,
            // repeatable.
            "2" => {
                self.buy_after_gates(
                    price, // 20ae:0b2f = 5
                    &[],
                    "^4Не хватает", // CS 0x8e4d `^4Не хватает`, 1000:be29
                    |g| {
                        // Debit 1000:be49, then 1000:be4d `mov ax,0x3` /
                        // 1000:be51 `call 0f78:114b`, dispatched over three
                        // compares. All three arms converge on 1000:beb4 and
                        // the roll changes NO state -- it is purely cosmetic,
                        // and it is still a draw.
                        match g.rng.below_at("1000:be51", 3) {
                            // 1000:be56 `cmp ax,0x0` / 1000:be59 `jnz 0xbe76`.
                            0 => term::println("^2Глинское? Чё за нафиг? А ладно."), // CS 0x8e5a `^2Глинское? Чё за нафиг? А ладно.`, 1000:be5b
                            // 1000:be76 `cmp ax,0x1` / 1000:be79 `jnz 0xbe96`.
                            1 => term::println("^2Пивко. Холодненькое."), // CS 0x8e7c `^2Пивко. Холодненькое.`, 1000:be7b
                            // 1000:be96 `cmp ax,0x2` / 1000:be99 `jnz 0xbeb4`.
                            2 => term::println("^2Ну чё по пиву?."), // CS 0x8e93 `^2Ну чё по пиву?.`, 1000:be9b
                            // 1000:be99's own target: no line, and the
                            // increment still runs. Unreachable from a
                            // `Random(3)`, and present because the original's
                            // dispatch is three equality tests, not an
                            // exhaustive three-way split.
                            _ => {}
                        }
                        // 1000:beb4 `inc [0x38c3]` -- a WORD count of
                        // half-litres, not a flag. The refusal path rejoins
                        // at 1000:be42 `jmp short 0xbeb8`, past this, so a
                        // failed purchase adds no beer.
                        g.player.beer_dl += 1;
                    },
                );
                true
            }
            // Row 3, Затемнённые очки. Setup 1000:beb8, key compare
            // 1000:bec2, miss 1000:bec7 `jnz 0xbf38`. Two gates.
            "3" => {
                // 1000:bec9 `cmp byte [0x38b3],0x0` / 1000:bece `jnz 0xbf1f`.
                let owned = self.dark_glasses;
                self.buy_after_gates(
                    price, // 20ae:0b30 = 10
                    // CS 0x8ed9 `^6У тебя есть очки от солнца.`, pushed at 1000:bf1f.
                    &[(owned, Some("^6У тебя есть очки от солнца."))],
                    "^4Не хватает бабок", // CS 0x8ea7 `^4Не хватает бабок`, 1000:bedb
                    |g| {
                        // 1000:bef6 `mov byte [0x38b3],0x1`; debit 1000:bf00.
                        // The flag is read outside this arm at the sheet's
                        // 1000:1cf8 and at the wander cop encounter's
                        // 1000:b7c6 `cmp byte [0x38b3],0x1`, which is where
                        // the glasses actually stop a fight.
                        g.dark_glasses = true;
                        term::println("^2Модные такие очки от солнца."); // CS 0x8eba `^2Модные такие очки от солнца.`, 1000:bf04
                    },
                );
                true
            }
            // Row 4, костюм abibas. Setup 1000:bf38, key compare 1000:bf42,
            // miss 1000:bf47 `jz 0xbf4c` over 1000:bf49 `jmp 0xbfe1`.
            "4" => {
                // 1000:bf4c `cmp byte [0x38b7],0x0` / 1000:bf51 `jnz 0xbfc8`.
                // ONE conjunct -- unlike `bmar` rows 5 and 6, whose
                // better-weapon gates AND three and two flags together.
                let better = self.wear_suit_adidas_38b7;
                // 1000:bf53 `cmp byte [0x38b4],0x0` / 1000:bf58 `jnz 0xbfad`.
                let owned = self.wear_suit_abibas_38b4;
                self.buy_after_gates(
                    price, // 20ae:0b31 = 15
                    &[
                        // CS 0x8f48 `^6У тебя есть более крутой костюм.`, pushed at 1000:bfc8.
                        (better, Some("^6У тебя есть более крутой костюм.")),
                        // CS 0x8f2e `^6У тебя уже есть костюм.`, pushed at 1000:bfad.
                        (owned, Some("^6У тебя уже есть костюм.")),
                    ],
                    "^4Не хватает денег", // CS 0x8ef9 `^4Не хватает денег`, 1000:bf65
                    |g| {
                        g.wear_suit_abibas_38b4 = true; // 1000:bf80
                                                        // Debit 1000:bf8a.
                        term::println("^2Теперь ты больше похож на гопа."); // CS 0x8f0c `^2Теперь ты больше похож на гопа.`, 1000:bf8e
                                                                            // 1000:bfa7 `inc [0x38b2]` -- the armour byte, +1,
                                                                            // unconditionally. The menu line's `Смягчает пинок
                                                                            // на 1` agrees; the number comes from the
                                                                            // instruction. Read outside this arm by the kick's
                                                                            // damage reduction at 1000:4769 and the gym's
                                                                            // recompute at 1000:e3a4.
                        g.player.armor += 1;
                    },
                );
                true
            }
            // Row 5, Понтовые бутсы. Setup 1000:bfe1, key compare 1000:bfeb,
            // miss 1000:bff0 `jz 0xbff5` over 1000:bff2 `jmp 0xc08e` -- and
            // 1000:c08e is row 6's DISTRICT GATE, not its setup.
            "5" => {
                // 1000:bff5 `cmp byte [0x38b8],0x0` / 1000:bffa `jnz 0xc075`.
                let better = self.wear_boots_pontovye_38b8;
                // 1000:bffc `cmp byte [0x38b5],0x0` / 1000:c001 `jnz 0xc05a`.
                let owned = self.wear_boots_38b5;
                self.buy_after_gates(
                    price, // 20ae:0b32 = 15
                    &[
                        // CS 0x8fad `^6У тебя бутсы по круче.`, pushed at 1000:c075.
                        (better, Some("^6У тебя бутсы по круче.")),
                        // CS 0x8f94 `^6У тебя такие уже есть.`, pushed at 1000:c05a.
                        (owned, Some("^6У тебя такие уже есть.")),
                    ],
                    "^4Нету на них денег", // CS 0x8f6d `^4Нету на них денег`, 1000:c00e
                    |g| {
                        g.wear_boots_38b5 = true; // 1000:c029
                                                  // Debit 1000:c033.
                        term::println("^2Зацени красовки."); // CS 0x8f81 `^2Зацени красовки.`, 1000:c037
                                                             // 1000:c050 `inc [0x38a8]` and 1000:c054
                                                             // `inc [0x38aa]` -- the damage range, +1/+1,
                                                             // unconditionally. The menu says only `Увеличивают
                                                             // урон`; the number comes from those two
                                                             // instructions.
                        g.player.dmg_min += 1;
                        g.player.dmg_max += 1;
                    },
                );
                true
            }
            // Row 6, Реальную кожанку. Span starts at the district gate
            // 1000:c08e; setup 1000:c098, key compare 1000:c0a2, miss
            // 1000:c0a7 `jz 0xc0ac` over 1000:c0a9 `jmp 0xc142`.
            "6" => {
                // 1000:c08e `cmp byte [0x3692],0x1` / 1000:c093 `ja 0xc098` /
                // 1000:c095 `jmp 0xc142`. A BUY-path district test, which no
                // `bmar` row has, and it prints nothing: 1000:c095 lands on
                // row 7's setup and the line falls through to the re-prompt.
                let below_district = self.district <= 1;
                // 1000:c0ac `cmp byte [0x38b9],0x0` / 1000:c0b1 `jnz 0xc129`.
                let better = self.wear_jacket_krutaya_38b9;
                // 1000:c0b3 `cmp byte [0x38b6],0x0` / 1000:c0b8 `jnz 0xc10e`.
                let owned = self.wear_jacket_38b6;
                self.buy_after_gates(
                    price, // 20ae:0b33 = 25
                    &[
                        (below_district, None),
                        // CS 0x9007 `^6Утебя есть кожанка круче.`, pushed at 1000:c129. The missing space
                        // after `У` is the original's.
                        (better, Some("^6Утебя есть кожанка круче.")),
                        // CS 0x8ff3 `^6Ты уже купил это.`, pushed at 1000:c10e.
                        (owned, Some("^6Ты уже купил это.")),
                    ],
                    "^4Не достаточно бабла", // CS 0x8fc8 `^4Не достаточно бабла`, 1000:c0c5
                    |g| {
                        g.wear_jacket_38b6 = true; // 1000:c0e0
                                                   // Debit 1000:c0ea.
                        term::println("^2Ну весь на понтах."); // CS 0x8fde `^2Ну весь на понтах.`, 1000:c0ee
                        g.player.armor += 2; // 1000:c107 add byte [0x38b2],0x2
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
                let owned = self.wear_suit_adidas_38b7;
                // 1000:c1aa `cmp byte [0x38b4],0x0` / 1000:c1af `jz 0xc1b7`.
                // Read BEFORE the arm runs because the flag this arm writes
                // is a different one (`20ae:38b7`), so nothing here observes
                // its own write.
                let has_abibas = self.wear_suit_abibas_38b4;
                self.buy_after_gates(
                    price, // 20ae:0b34 = 30
                    // CS 0x9036 `^6У тебя уже есть этот костюм.`, pushed at 1000:c1be.
                    &[(owned, Some("^6У тебя уже есть этот костюм."))],
                    "^4Не хватает денег", // CS 0x8ef9 `^4Не хватает денег`, 1000:c168 -- row 4's literal
                    |g| {
                        g.wear_suit_adidas_38b7 = true; // 1000:c183
                                                        // Debit 1000:c18d.
                        term::println("^2Чистый гопник."); // CS 0x9025 `^2Чистый гопник.`, 1000:c191
                                                           // The UPGRADE SPLIT: 1000:c1b1 `inc [0x38b2]` when
                                                           // the abibas suit is already owned, 1000:c1b7
                                                           // `add byte [0x38b2],0x2` when it is not, rejoining
                                                           // at 1000:c1b5 `jmp short 0xc1bc`. Either way the
                                                           // player ends on +2 of suit armour, whichever order
                                                           // the two rows were bought in.
                        g.player.armor += if has_abibas { 1 } else { 2 };
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
                let owned = self.wear_boots_pontovye_38b8;
                // 1000:c249 `cmp byte [0x38b5],0x0` / 1000:c24e `jz 0xc25a`.
                let has_boots = self.wear_boots_38b5;
                self.buy_after_gates(
                    price, // 20ae:0b35 = 30
                    &[
                        (below_district, None),
                        // CS 0x8f94 `^6У тебя такие уже есть.`, pushed at 1000:c266 -- row 5's literal.
                        (owned, Some("^6У тебя такие уже есть.")),
                    ],
                    "^4Нету на них денег", // CS 0x8f6d `^4Нету на них денег`, 1000:c207 -- row 5's too
                    |g| {
                        g.wear_boots_pontovye_38b8 = true; // 1000:c222
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
                let owned = self.wear_jacket_krutaya_38b9;
                // 1000:c2f1 `cmp byte [0x38b6],0x0` / 1000:c2f6 `jz 0xc2ff`.
                let has_jacket = self.wear_jacket_38b6;
                self.buy_after_gates(
                    price, // 20ae:0b36 = 50
                    &[
                        (below_district, None),
                        // CS 0x8ff3 `^6Ты уже купил это.`, pushed at 1000:c306 -- row 6's literal.
                        (owned, Some("^6Ты уже купил это.")),
                    ],
                    "^4Не достаточно бабла", // CS 0x8fc8 `^4Не достаточно бабла`, 1000:c2af -- row 6's too
                    |g| {
                        g.wear_jacket_krutaya_38b9 = true; // 1000:c2ca
                                                           // Debit 1000:c2d4.
                        term::println("^2Ну крутой, сдохнуть можно!"); // CS 0x906c `^2Ну крутой, сдохнуть можно!`, 1000:c2d8
                                                                       // The UPGRADE SPLIT: 1000:c2f8
                                                                       // `add byte [0x38b2],0x2` with the lesser jacket
                                                                       // owned, 1000:c2ff `add byte [0x38b2],0x4` without,
                                                                       // rejoining at 1000:c2fd `jmp short 0xc304`.
                        g.player.armor += if has_jacket { 2 } else { 4 };
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
    fn run_combat(
        &mut self,
        mut enemy: Fighter,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        if self.fight_log.is_some() {
            let at = self.rng.draws_logged();
            if let Some(log) = self.fight_log.as_mut() {
                log.fights.push((at, enemy.clone()));
            }
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
            if self.fight_log.is_some() {
                let draws_before = self.rng.draws_logged();
                let state = PromptState {
                    fight: self.fight_log.as_ref().map_or(0, |l| l.fights.len()),
                    draws_before,
                    player_hp: self.player.hp,
                    player_hpmax: self.player.hpmax,
                    enemy_hp: enemy.hp,
                    enemy_hpmax: enemy.hpmax,
                    player_broken_jaw: self.player.broken_jaw,
                    player_broken_leg: self.player.broken_leg,
                    enemy_broken_jaw: enemy.broken_jaw,
                    enemy_broken_leg: enemy.broken_leg,
                };
                if let Some(log) = self.fight_log.as_mut() {
                    log.prompts.push(state);
                }
            }
            let Some(line) = lines.next() else {
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
            // `Command::Walk`).
            if line.trim().eq_ignore_ascii_case("run") {
                fled = self.flee();
            }

            // 1000:4afb / 1000:4b00 -- FUN_1000_3d11 calls FUN_1000_29c4, the
            // same routine `entry` calls at 1000:e966, with its own DS:3a72.
            match cmd {
                Command::Drink => self.beer(Beer::One),
                Command::BingeDrink => self.beer(Beer::Binge),
                // 1000:4b0d, token file 0x4D81 -> the arm at 1000:4b17.
                Command::Joint => self.smoke(Joint::Fight),
                // 1000:4c2e, token CS 0x359f -> 1000:4c35 `call 0x1a03`, the
                // PLAYER's sheet (Task 16, `docs/re/character-sheet.md`).
                Command::Stats => self.show_stats(),
                // 1000:4c42, token CS 0x35a1 -> 1000:4c49 `call 0x1348`, the
                // ENEMY's sheet -- a different function, settled in Task 17.
                // `FUN_1000_1348` references no address in the player's
                // record at all, so `print_enemy_block` is the right callee
                // here and `show_stats` would be the wrong one.
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
            if line.trim().eq_ignore_ascii_case("e") {
                self.last_enemy = Some(enemy);
                self.running = false;
                return Ok(());
            }

            // 1000:4c64 `cmp word [0x3c80],1` / `jl 0x4ca0` guards the SECOND
            // `k` compare at 1000:4c75, so the countdown only ticks once the
            // backup has been called.
            if backup.count() >= 1 && cmd == Command::Fight && backup.tick_on_attack() {
                // 1000:4c87, CS 0x35a6 -- the copy WITHOUT the trailing dot.
                term::println("^2Подошли пацаны - Ща начнется!");
            }

            // 1000:4caa, token CS 0x35c6 -> the arm at 1000:4cb4.
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

            // 1000:4ea8, token CS 0x3714 -> the arm at 1000:4eb2. There is no
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
                self.running = false;
                return Ok(());
            }
            if self.hospital_rescue() {
                return Ok(());
            }
            // 1000:5053, file 0x5127, then FUN_1000_074b and the RTL's
            // `mov ah,0x4c` / `int 0x21`: death ends the process.
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
        let short_of_the_threshold = self.progress.xp + award < self.progress.threshold;
        progress::apply_levels(
            &mut self.progress,
            &mut self.player,
            &mut self.rng,
            award,
            false,
        );
        if short_of_the_threshold {
            term::println("^6Ты запинал слишком слабого мудака для увеличения понтовости");
            term::println(&text::fill(
                "^6Сейчас у тебя # качков опыта, А для прокачки надо #",
                &[self.progress.xp as i64, self.progress.threshold as i64],
            ));
        }
        self.claim_spoils(&enemy);

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
    /// **The four calls at `ae27`..`ae39` are deliberately NOT ported --
    /// the brief's escape hatch.** `FUN_1000_11c2` was traced for this task
    /// (no `docs/re/` file cited it before): 50 instructions, 178 bytes
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
    /// The obstacle is `FUN_1000_3d11`'s own `param_1` (the fight function's
    /// `bp+4`), which [`Game::run_combat`] does not model at all:
    /// * `1000:51b9`..`1000:51e9`, the XP award, is skipped when `param_1`
    ///   is 3 or 4 (`docs/re/combat.md`, "The victory block") --
    ///   `run_combat` currently awards XP unconditionally.
    /// * `1000:5085 cmp byte [bp+0x4],0x4` selects an entirely separate
    ///   victory ending for `param_1 == 4` -- `FUN_1000_074b(1)`, the
    ///   end-of-game banner (file `0x1DBF`) -- which has never been traced
    ///   by this project (`docs/re/wander.md`: "Whether `FUN_1000_3d11(4)`
    ///   returns is not traced here").
    ///
    /// Porting either fight correctly needs both of those traced and
    /// `run_combat`'s signature widened first; that is a combat-dispatch
    /// task, not a flag-setter one. `FUN_1000_3d11`'s `param_1` handling
    /// (not `1000:11c2`, which this task fully settled) is recorded as open
    /// in `docs/re/gaps.md` rather than guessed at here, per "Do not port a
    /// call whose callee you have not read" -- `1000:3d11`'s callee IS read
    /// (it is `run_combat` itself), but not for this argument.
    ///
    /// **What genuinely runs every turn is `1000:ae18`'s arm, not this
    /// one**, and the difference is the whole of the remaining divergence.
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
        let _ = lines.next();
        term::println("^1Ты пробрался в универ, в тёмный ректорский кабинет...");
        term::println("^1А вот и он...");
        self.rector_showdown = true;
        self.places.mark_found(Location::Den);
    }

    /// Begin recording the fight channels, discarding anything recorded.
    ///
    /// The same design as [`crate::rng::Rng::start_log`], and for the same
    /// reason: `data/combat_trace.json` carries two channels the draw stream
    /// cannot see -- the enemy record each fight was entered with
    /// (`1000:3d11`) and both fighters' hp and break flags at every `Битва\`
    /// prompt (`1000:441d`) -- and a replay that could not produce them would
    /// leave `Fighter::broken_jaw`/`broken_leg` asserted by nothing, which is
    /// where they stood before Task 13. `None` for every game the binary
    /// builds, so a real session allocates nothing.
    pub fn start_fight_log(&mut self) {
        self.fight_log = Some(FightLog::default());
    }

    /// Take the recorded fight channels and stop recording.
    pub fn take_fight_log(&mut self) -> FightLog {
        self.fight_log.take().unwrap_or_default()
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
        self.pontovost_street -= 10;
        // Round(hpmax / 5 * 3), computed exactly rather than through two
        // truncating divisions: `round_half(x)` rounds `x / 2` half away from
        // zero, so feeding it `6 * hpmax / 5` as a doubled numerator gives
        // `Round(3 * hpmax / 5)`. (The half case never arises -- `3h/5` has
        // fractional part 0, .2, .4, .6 or .8 -- but the rounding is written
        // the original's way rather than assumed away.)
        let bill = Self::round_half(6 * i32::from(self.player.hpmax) / 5);
        self.player.money -= bill;
        self.player.hp = self.player.hpmax;
        if self.player.broken_jaw || self.player.broken_leg {
            self.player.money -= 7;
            self.player.broken_jaw = false;
            self.player.broken_leg = false;
        }
        if self.player.money < 0 {
            self.pontovost_street += self.player.money;
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
            self.flag_3693,
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
        self.player.money += enemy.money;
        self.player.junk += enemy.junk;
        term::println("^1Пиво победителю!"); // file 0x52FE
        self.player.hp = (self.player.hp + 5).min(self.player.hpmax);
        self.pontovost_street += i32::from(enemy.class) + 1 + i32::from(enemy.level) / 3;
        // 1000:5295..1000:52cc. `[0x3692]` is the district; the level is
        // measured within it, so three levels into a district opens the den.
        if !self.places.is_found(Location::Den)
            && i32::from(self.player.level) - (i32::from(self.district) - 1) * 10 >= 3
        {
            self.places.mark_found(Location::Den);
            term::println(
                "^1Поновость улутшилась на столько, что тебе можно заходить в местный притон!",
            );
        }
        if self.rng.below_at("1000:52d5", 30) == 0 {
            self.grant_oneshot_gift();
        }
        // 1000:53f7..1000:5444 -- the Нарк's joints.
        let roll = self
            .rng
            .below_at("1000:5402", u16::from(self.district) * 25);
        if i32::from(self.player.luck) >= i32::from(roll) && enemy.class == 2 {
            self.player.joints += self.rng.below_at("1000:5427", 3);
            term::println("^1А у нарка был косячок"); // file 0x540B
        }
        // 1000:5449..1000:57cc -- the class-keyed item table.
        let roll = self
            .rng
            .below_at("1000:5454", u16::from(self.district) * 40);
        if i32::from(self.player.luck) < i32::from(roll) {
            return;
        }
        match enemy.class {
            1 => self.spoil_charm(),
            3..=6 => self.spoil_club(),
            7 => self.spoil_glasses(),
            9 => self.spoil_blade(),
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
    /// still unfired -- `1000:52e1`/`1000:52e8`/`1000:52ed` are three `je`s
    /// onto one common target.
    fn grant_oneshot_gift(&mut self) {
        if !self.oneshot_gift_1 || !self.oneshot_gift_2 || !self.ring_gospodi_pomilui {
            term::println("^1Оба на! Колечко! Вот свезло, так свезло!");
        }
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
                self.player.dmg_min += 1; // 1000:534d..1000:5361
            }
            self.oneshot_gift_1 = true;
        } else if !self.oneshot_gift_2 {
            term::println("^1\"Мега Кольцо\"!");
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
            term::println("^1Ваще полезное кольцо \"Господи помилуй\"");
            term::println("^1Восст. жизни - 3, 5% - самозарост переломов");
            self.ring_gospodi_pomilui = true;
        }
    }

    /// Enemy class 1 (Нефор): `1000:547e`..`1000:5512`, `Random(3)`.
    fn spoil_charm(&mut self) {
        match self.rng.below_at("1000:5482", 3) {
            0 => {
                // 1000:548c gate, 1000:5493 `add [0x38a4],2`, 1000:54b1 flag.
                if !self.charm_krestik_38bd {
                    self.player.luck += 2;
                    term::println("^1Ты нашёл крестик: удача +2");
                    self.charm_krestik_38bd = true;
                }
            }
            1 => {
                // 1000:54bd gate, 1000:54c4 `inc [0x38a4]`, 1000:54e1 flag.
                if !self.charm_ring_38be {
                    self.player.luck += 1;
                    term::println("^1Ты нашёл кольцо \"Господи спаси\": удача +1");
                    self.charm_ring_38be = true;
                }
            }
            _ => {
                // 1000:54ed gate, 1000:550d flag. No stat change.
                if !self.has_mobile {
                    term::println("^1Ты нашёл мобилу");
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
        match self.rng.below_at("1000:5530", 2) {
            0 => {
                if self.weapon_kastet_38ba {
                    return;
                }
                self.weapon_kastet_38ba = true; // 1000:5541
                term::println("^1Ты надыбал кастет(урон+2)");
                // 1000:555f/1000:5566/1000:556d -- ножик, дубинка, тесак.
                if !self.weapon_nozhik_38c2 && !self.weapon_dubinka_394b && !self.weapon_tesak_394c
                {
                    self.player.dmg_min += 2; // 1000:5574
                    self.player.dmg_max += 2;
                } else {
                    term::println("^6Но у тебя есть более мощное оружие");
                }
            }
            _ => {
                if self.weapon_dubinka_394b {
                    return;
                }
                self.weapon_dubinka_394b = true; // 1000:55a7
                term::println("^1Ты отобрал у врага дубинку(урон+4)");
                // 1000:55c5/1000:55cc -- ножик, тесак.
                if self.weapon_nozhik_38c2 || self.weapon_tesak_394c {
                    term::println("^6Но у тебя есть более мощное оружие");
                } else if self.weapon_kastet_38ba {
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
        match self.rng.below_at("1000:5617", 2) {
            0 => {
                // 1000:5621 gate, 1000:5628 flag. No stat change.
                if !self.dark_glasses {
                    self.dark_glasses = true;
                    term::println("^1Ты нашёл тёмные очки.");
                }
            }
            _ => {
                // 1000:564d gate, 1000:566d flag.
                if !self.has_mobile {
                    term::println("^1Ты нашёл мобилу");
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
        match self.rng.below_at("1000:5681", 2) {
            0 => {
                if self.weapon_nozhik_38c2 {
                    return;
                }
                self.weapon_nozhik_38c2 = true; // 1000:5698
                term::println("^1Ты нашел ножик(урон+6).");
                // 1000:56bc..1000:56cd: al := (394b == 0); `cmp al,[0x38ba]`.
                if !self.weapon_dubinka_394b == self.weapon_kastet_38ba {
                    self.player.dmg_min += 4; // 1000:56cf
                    self.player.dmg_max += 4;
                }
                if self.weapon_dubinka_394b {
                    self.player.dmg_min += 2; // 1000:56e0
                    self.player.dmg_max += 2;
                }
                if !self.weapon_kastet_38ba && !self.weapon_dubinka_394b && !self.weapon_tesak_394c
                {
                    self.player.dmg_min += 6; // 1000:56ff
                    self.player.dmg_max += 6;
                }
                if self.weapon_tesak_394c {
                    term::println("^6Но утебя есть тесак который круче."); // file 0x5516
                }
            }
            _ => {
                if self.weapon_tesak_394c {
                    return;
                }
                self.weapon_tesak_394c = true; // 1000:573e
                term::println("^1Ты нашел тесак(урон+9)!!! - ужасное оружие.");
                // 1000:5762..1000:577a: al := (394b == 0 && 38c2 == 0).
                if (!self.weapon_dubinka_394b && !self.weapon_nozhik_38c2)
                    == self.weapon_kastet_38ba
                {
                    self.player.dmg_min += 7; // 1000:577c
                    self.player.dmg_max += 7;
                }
                if self.weapon_dubinka_394b && !self.weapon_nozhik_38c2 {
                    self.player.dmg_min += 5; // 1000:5794
                    self.player.dmg_max += 5;
                }
                if self.weapon_nozhik_38c2 {
                    self.player.dmg_min += 3; // 1000:57a5
                    self.player.dmg_max += 3;
                }
                if !self.weapon_kastet_38ba && !self.weapon_dubinka_394b && !self.weapon_nozhik_38c2
                {
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
        if *prompts_seen != 5 {
            return;
        }
        if self.rng.below_at("1000:4135", 10) != 0 {
            return;
        }
        let which = self.rng.below_at("1000:4145", 18);
        // The eighteen lines at code offsets 0x2e92..0x314d (files
        // 0x4762..0x4A1D), in the order the `cmp ax,N` chain at 1000:414a
        // onwards tests them. Two are built from a name: 4 splices the
        // PLAYER'S RANK name (`[0x389c] * 0x100 + 0x2e`, the DS:002e table
        // `data/enemies.json` carries) and 17 the player's own name
        // (`DS:379c`).
        match which {
            0 => term::println("Зрители:^6Мочи его, мочи!"),
            1 => term::println("Зрители:^6Врежь ему!"),
            2 => term::println("Зрители:^6Блин долго ты ещё будешь мудиться?"),
            3 => term::println("Зрители:^6Да вы только посмотрите на эти пинки!"),
            4 => {
                term::print("Зрители:^6Не подкачай ");
                term::print(&Self::rank_name(self.player.class));
                term::println(", я на тебя трёшку поставил!");
            }
            5 => term::println("Зрители:^6Чё-тут за батва?"),
            6 => term::println("Зрители:^6Я знаю вон того мудака, он уже нескольких запинал!"),
            7 => term::println("Зрители:^6Чё так слабо бьёшь?! Пинай сильнее!"),
            8 => {
                term::println("Зрители:^6Дерьмово дерётесь придурки");
                term::println("^2А ты: Заткнись мудак, а то щас тебя запинаю!");
            }
            9 => term::println(
                "Зрители:^6Да, а помнишь мы вчера также одного пинали, пинали.. \
                 А потом подошла его братва..",
            ),
            10 => term::println("Зрители:^6Это чё реслинг?"),
            11 => term::println("Зрители:^6Двинь ему в рыло!"),
            12 => term::println("Зрители:^6И куда менты смотрят?"),
            13 => term::println("Зрители:^6Пинай!"),
            14 => term::println("Зрители:^6Врежь гаду!"),
            15 => term::println("Зрители:^6Господа делайте ваши ставки!"),
            16 => term::println("Зрители:^6Ну чё там? Какой счет?"),
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
    fn rank_name(class: u16) -> String {
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
                Some(0) => term::println("^2Точный удар!!!"),
                Some(1) => term::println("^2Не хило приложил!!!"),
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
                Some(0) => term::println("^4Враг:Сдохни урод!!"),
                Some(1) => term::println("^4Тебе не хило врезали!"),
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
            Joint::Street => "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косякова",
            // file 0x4DF0, loaded by `bf 20 35` at 1000:4bb8.
            Joint::Fight => "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косяков",
        }
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

        let mut g = game();
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

    /// `1000:b76a`'s three arms, and which of them starts a fight.
    ///
    /// Driven through `cop_encounter` directly because reaching it from
    /// `walk` needs the generator to roll class 8, which is a seed hunt
    /// rather than a test. The input iterator is the assertion: the two
    /// no-fight arms read **no** line (there is no prompt on this path at
    /// all), while the `^4Запалил!` arm hands straight to combat, which
    /// does read one.
    #[test]
    fn the_cop_encounter_fights_only_when_luck_loses_and_the_glasses_are_off() {
        // Takes `&mut Rng` rather than `&mut Game`: the only thing it does
        // to the game is start the RNG log, which only the first of the
        // three cases below goes on to read.
        let cop = |rng: &mut Rng| {
            rng.start_log();
            Fighter {
                class: 8,
                name: "Мент".to_string(),
                hp: 10,
                hpmax: 10,
                ..Fighter::default()
            }
        };

        // Luck wins the compare (1000:b7ab): no fight. The tattoo is set
        // here on purpose: `1000:b784`..`1000:b791` has no
        // `cmp byte [0x38bc],1`, so unlike `1000:b5f1` this roll must NOT
        // be halved -- which is what makes the other test's "only" true.
        let mut g = game();
        g.player.luck = 10_000;
        g.prison_tattoo = true;
        let e = cop(&mut g.rng);
        let mut lines = input(&["run", "run"]);
        g.cop_encounter(e, &mut lines).unwrap();
        assert_eq!(lines.count(), 2, "the stealth arm must read no line");
        let log = g.rng.take_log();
        assert_eq!(log.len(), 1, "exactly one draw, 1000:b792");
        assert_eq!(log[0].site, "1000:b792");
        assert_eq!(
            log[0].n, 22,
            "district 1 -> 1 * 7 + 15, and the tattoo must not halve it"
        );

        // Luck loses but the тёмные очки are on (1000:b7cd): still no fight.
        let mut g = game();
        g.player.luck = 0;
        g.dark_glasses = true;
        let e = cop(&mut g.rng);
        let mut lines = input(&["run", "run"]);
        g.cop_encounter(e, &mut lines).unwrap();
        assert_eq!(lines.count(), 2, "the glasses arm must read no line");

        // Luck loses with no glasses (1000:b801): the fight starts.
        let mut g = game();
        g.player.luck = 0;
        let e = cop(&mut g.rng);
        let mut lines = input(&["run", "run"]);
        g.cop_encounter(e, &mut lines).unwrap();
        assert_eq!(
            lines.count(),
            1,
            "^4Запалил! must reach FUN_1000_3d11, which prompts"
        );
    }

    /// The зоновская наколка halves `1000:b5f1`'s `n` and nothing else's.
    /// Asserted on the `n` the port actually pushes, over a whole walk, so a
    /// regression in either direction shows up.
    #[test]
    fn the_prison_tattoo_halves_only_the_ordinary_notice_roll() {
        for (tattoo, want) in [(false, 22u16), (true, 11u16)] {
            let mut seen = None;
            // Walk until a bucket-3 turn produces an ordinary encounter.
            for seed in 0..400u32 {
                let mut g = game();
                g.prison_tattoo = tattoo;
                g.rng = Rng::new(seed);
                g.rng.start_log();
                g.walk(&mut input(&["run", "run", "run", "run"])).unwrap();
                if let Some(d) = g.rng.take_log().iter().find(|d| d.site == "1000:b5f1") {
                    seen = Some(d.n);
                    break;
                }
            }
            assert_eq!(
                seen,
                Some(want),
                "tattoo = {tattoo}: 1000:b5f1's n (district 1)"
            );
        }
    }

    /// `1000:48dc`'s `run`, both arms that leave the fight and the one that
    /// does not, and the fact that none of them draws.
    #[test]
    fn run_leaves_a_fight_without_spending_a_draw() {
        let enemy = || Fighter {
            name: "Дохляк".to_string(),
            hp: 50,
            hpmax: 50,
            ..Fighter::default()
        };

        // Level 0 (1000:4ade): leaves, and reads exactly one line.
        let mut g = game();
        g.player.level = 0;
        g.rng.start_log();
        let mut lines = input(&["run", "run", "run"]);
        g.run_combat(enemy(), &mut lines).unwrap();
        assert_eq!(lines.count(), 2, "one line consumed, then the fight ended");
        assert!(
            g.rng.take_log().is_empty(),
            "no arm of 1000:48eb..1000:4afb calls Random"
        );

        // A broken leg (1000:490e): stays in the fight, so every line is
        // consumed and the loop only ends when the input runs out.
        let mut g = game();
        g.player.broken_leg = true;
        let mut lines = input(&["run", "run", "run"]);
        g.run_combat(enemy(), &mut lines).unwrap();
        assert_eq!(
            lines.count(),
            0,
            "1000:4915 re-prompts rather than leaving the fight"
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

    fn draws_at(g: &mut Game, site: &str) -> usize {
        g.rng.take_log().iter().filter(|d| d.site == site).count()
    }

    /// The whole `v` sequence, measured on the two draw sites only the
    /// backup block owns. `v` places the call and three `k`s take the
    /// counter 1 -> 2 -> 3; `1000:4d9d` opens on the prompt the counter
    /// reaches 3, so the gopota swing on that prompt and on every one after.
    ///
    /// The control is the same script with the den flag clear: `1000:4cb4`
    /// refuses, the counter never leaves 0 and neither site ever fires.
    #[test]
    fn v_starts_a_countdown_that_k_ticks_and_then_the_gopota_swing() {
        let script = ["v", "k", "k", "k"];

        let mut g = game_with_gopota();
        g.rng.start_log();
        g.run_combat(punchbag(), &mut input(&script)).unwrap();
        let log = g.rng.take_log();
        let n = |site: &str| log.iter().filter(|d| d.site == site).count();
        assert_eq!(n("1000:4db7"), 2, "the last two prompts have the gopota");
        assert_eq!(n("1000:4e16"), 2, "the attrition coin, once per swing");
        // Order matters: 1000:4db7 is the damage roll and 1000:4e16 the
        // attrition, in that order, every time.
        let backup: Vec<&str> = log
            .iter()
            .map(|d| d.site)
            .filter(|s| *s == "1000:4db7" || *s == "1000:4e16")
            .collect();
        assert_eq!(backup, ["1000:4db7", "1000:4e16", "1000:4db7", "1000:4e16"]);

        let mut g = game_with_gopota();
        g.places = Places::from_bytes(&[0u8; 7]);
        g.rng.start_log();
        g.run_combat(punchbag(), &mut input(&script)).unwrap();
        assert_eq!(draws_at(&mut g, "1000:4db7"), 0, "no den, no gopota");
    }

    /// `1000:4cc8` -- the cred gate is `cred >= district * 10 + 10`, and the
    /// `jnle` makes the boundary itself pass. Measured on whether the
    /// countdown started, one either side of the boundary, in two districts
    /// so the `district * 10` term is exercised and not just the `+ 10`.
    #[test]
    fn the_backup_cred_gate_moves_with_the_district() {
        for (district, need) in [(1u8, 20i32), (3, 40)] {
            for (cred, expect_call) in [(need - 1, false), (need, true)] {
                let mut g = game_with_gopota();
                g.district = district;
                g.pontovost_street = cred;
                g.rng.start_log();
                // `v` then three `k`s: if the call went through, the gopota
                // arrive and their damage roll fires.
                g.run_combat(punchbag(), &mut input(&["v", "k", "k", "k"]))
                    .unwrap();
                let fired = draws_at(&mut g, "1000:4db7") > 0;
                assert_eq!(
                    fired, expect_call,
                    "district {district}, cred {cred} (needs {need})"
                );
            }
        }
    }

    /// `1000:4cdb` -- the mobile phone stores 3 outright, and `1000:4d93` is
    /// DOWNSTREAM of the `v` arm on the same straight line, so the gopota
    /// swing in the very prompt the call was placed in and again in the next
    /// one. Without a phone the same script leaves the counter at 1 and
    /// neither prompt swings.
    #[test]
    fn the_mobile_phone_puts_the_gopota_in_the_fight_at_once() {
        for (phone, want) in [(false, 0usize), (true, 2)] {
            let mut g = game_with_gopota();
            g.has_mobile = phone;
            g.rng.start_log();
            // `v`, then one line no compare matches -- so the only thing that
            // can draw at 1000:4db7 is the backup block itself.
            g.run_combat(punchbag(), &mut input(&["v", "zzz"])).unwrap();
            assert_eq!(draws_at(&mut g, "1000:4db7"), want, "phone {phone}");
        }
    }

    /// `[1000:4d93, 1000:4e9e)` is between the `v` arm and the `f` compare on
    /// the dispatcher's straight line, not inside either -- so once the
    /// gopota have arrived they swing on a line the chain never matched.
    #[test]
    fn the_gopota_swing_on_a_prompt_that_matched_no_verb_at_all() {
        let mut g = game_with_gopota();
        g.has_mobile = true;
        g.player.level = 0; // so the closing `run` costs nothing
        g.rng.start_log();
        // `zzz` and `qqq` match no compare at all; `wes` is a DEALERS verb,
        // compared at `1000:ced8` against `entry`'s buffer and never here.
        // The closing `run` ends the fight so `last_enemy` is recorded.
        g.run_combat(punchbag(), &mut input(&["v", "zzz", "qqq", "run"]))
            .unwrap();
        let log = g.rng.take_log();
        assert_eq!(
            log.iter().filter(|d| d.site == "1000:4db7").count(),
            4,
            "the call prompt and the three after it, whatever was typed"
        );
        // ... and the enemy really lost the hp those rolls bought. District
        // 1: `3 + Random(4)` per swing, armour 0, so 12..=24 over four.
        let left = g
            .last_enemy
            .as_ref()
            .expect("the fight recorded an enemy")
            .hp;
        assert!(
            (500 - 24..=500 - 12).contains(&left),
            "enemy hp {left} is outside four district-1 backup blows"
        );
    }

    /// `1000:507b` `cmp word [0x3962],0` / `jle 0x5085` is read AFTER the
    /// whole chain and BEFORE `1000:5838`'s test of the flee flag, so a `run`
    /// in the prompt where the gopota landed the killing blow is a VICTORY,
    /// not an escape. `run` is compared at `1000:48e1` and the backup block
    /// starts at `1000:4d93`, so both really do happen in the one prompt.
    ///
    /// Marked by the victory block's own `Random(30)` at `1000:52d5`, which
    /// no other path in the function reaches.
    #[test]
    fn fleeing_in_the_prompt_the_gopota_win_is_still_a_victory() {
        // `strength` is there only so `1000:51b9`'s award (the sum of the
        // enemy's four stats) is non-zero and the kill is visible in the XP.
        let scenario = |seed: u32, hp: u16| {
            let mut g = game_with_gopota();
            g.rng = Rng::new(seed);
            g.has_mobile = true;
            g.rng.start_log();
            let enemy = Fighter {
                hp,
                hpmax: 50,
                strength: 4,
                ..punchbag()
            };
            g.run_combat(enemy, &mut input(&["v", "run"])).unwrap();
            let log = g.rng.take_log();
            let swings = log.iter().filter(|d| d.site == "1000:4db7").count();
            let victory = log.iter().any(|d| d.site == "1000:52d5");
            (swings, victory, g.progress.xp)
        };

        // District 1's backup blow is `3 + Random(4)`, so 3..=6. At 11 hp the
        // enemy always survives the first swing and some seeds kill it with
        // the second -- which is the prompt `run` is typed in.
        let seed = (0..2000u32)
            .find(|&s| scenario(s, 11) == (2, true, 4))
            .expect("some seed kills the enemy on the second backup swing");

        let (swings, victory, xp) = scenario(seed, 11);
        assert_eq!(swings, 2, "one swing per prompt, and the second killed");
        assert!(victory, "1000:507b is read before 1000:5838");
        assert_eq!(xp, 4, "the enemy's four stats were awarded");

        // Control, same seed: an enemy the gopota cannot kill in two swings
        // leaves by `1000:4af7` and never reaches the victory block, so the
        // difference above is the enemy's hp and nothing else.
        let (swings, victory, xp) = scenario(seed, 500);
        assert_eq!(swings, 2);
        assert!(!victory, "the enemy is still up, so the flee flag wins");
        assert_eq!(xp, 0);
    }

    /// `1000:4e79` -- the gopota bill `district * 5` of street cred per
    /// swing and walk out the moment it is not positive.
    #[test]
    fn the_gopota_leave_when_the_street_cred_runs_out() {
        let mut g = game_with_gopota();
        g.has_mobile = true;
        g.district = 1;
        // 20 clears 1000:4cc8's gate for district 1, and then four swings at
        // 5 apiece take it to exactly 0.
        g.pontovost_street = 20;
        g.rng.start_log();
        g.run_combat(punchbag(), &mut input(&["v", "z", "z", "z", "z", "z", "z"]))
            .unwrap();
        assert_eq!(g.pontovost_street, 0);
        assert_eq!(
            draws_at(&mut g, "1000:4db7"),
            4,
            "four swings at 5 cred each, then 1000:4e82 zeroes the counter"
        );
    }

    /// `1000:4eb2` and `1000:4ebc`/`1000:4ec3` -- the two gates that make
    /// `f` do nothing, measured on the draw count and on the magazine.
    #[test]
    fn shooting_needs_a_pistol_and_somewhere_it_is_allowed() {
        let cases = [
            (false, false, false, 0usize),
            (true, false, false, 0),
            (true, true, false, 1),
            (true, false, true, 1),
        ];
        for (owned, silencer, flag_3693, want_draws) in cases {
            let mut g = game();
            g.pistol = combat_dispatch::Pistol {
                owned,
                silencer,
                cartridges: 6,
            };
            g.flag_3693 = flag_3693;
            g.rng.start_log();
            g.run_combat(punchbag(), &mut input(&["f"])).unwrap();
            let fired = draws_at(&mut g, "1000:4ef5");
            assert_eq!(
                fired, want_draws,
                "owned {owned}, silencer {silencer}, 3693 {flag_3693}"
            );
            assert_eq!(
                g.pistol.cartridges,
                6 - want_draws as i16,
                "1000:4eed spends one only when the shot is taken"
            );
        }
    }

    /// `1000:4ee6` -- an empty magazine is its own refusal, and it must not
    /// take the count below zero however often `f` is typed.
    #[test]
    fn an_empty_magazine_refuses_without_drawing_or_going_negative() {
        let mut g = game();
        g.pistol = combat_dispatch::Pistol {
            owned: true,
            silencer: true,
            cartridges: 1,
        };
        g.rng.start_log();
        g.run_combat(punchbag(), &mut input(&["f", "f", "f", "f"]))
            .unwrap();
        assert_eq!(g.pistol.cartridges, 0);
        assert_eq!(
            draws_at(&mut g, "1000:4ef5"),
            1,
            "only the first `f` had a cartridge to spend"
        );
    }

    /// The shot lands on the enemy record, and its 20..=29 is subtracted with
    /// no armour term (`1000:4f28`) -- so an enemy in full armour loses
    /// exactly as much as a naked one from the same seed.
    #[test]
    fn the_pistol_ignores_the_enemy_armour() {
        let hit = |armor: u16| {
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
            g.run_combat(enemy, &mut input(&["f", "run"])).unwrap();
            g.last_enemy.as_ref().unwrap().hp
        };
        let bare = hit(0);
        assert!((500 - 29..=500 - 20).contains(&bare), "hp {bare}");
        assert_eq!(hit(60), bare, "1000:4f28 has no `armour div 3` term");
    }

    /// `1000:4c5d` `xor ax,ax` / `call 0f78:0116` is `Halt(0)`: `e` at the
    /// fight prompt leaves the whole game, not the fight, and reads no
    /// further line.
    ///
    /// **`exit` must NOT.** `crate::commands::parse` folds `e` and `exit`
    /// into one `Command::Quit` because `entry` dispatches both
    /// (`1000:edfa`, `1000:ede9`), and `FUN_1000_3d11` compares only `e`
    /// (CS `0x35a4` at `1000:4c56`). The shortstring `exit` sits at exactly
    /// one image offset, CS `0xab1e`, referenced only by `1000:ede9`, so it
    /// is never materialised inside the fight function and falls through the
    /// chain like any other unmatched line. Typing only `e` cannot catch a
    /// regression here, which is why both spellings are scripted.
    #[test]
    fn e_at_the_fight_prompt_halts_the_game_and_exit_does_not() {
        let mut g = game();
        let mut lines = input(&["e", "k", "k"]);
        g.run_combat(punchbag(), &mut lines).unwrap();
        assert!(!g.running, "1000:4c5f ends the process");
        assert_eq!(lines.count(), 2, "nothing after `e` is read");
        assert!(g.last_enemy.is_some(), "the fight still recorded its enemy");

        // Case folding: 1000:4431 `call 0eed:0x216` runs on the buffer before
        // any compare, so `E` is the same verb.
        let mut g = game();
        let mut lines = input(&["E", "k", "k"]);
        g.run_combat(punchbag(), &mut lines).unwrap();
        assert!(!g.running);
        assert_eq!(lines.count(), 2);

        // `exit` reaches no handler at all. The closing `run` (level 0, so
        // no penalty) is what ends the fight, and it is the discriminator:
        // if `exit` still halted, the game would be stopped and the `run`
        // never read.
        let mut g = game();
        let mut lines = input(&["exit", "exit", "run"]);
        g.rng.start_log();
        g.run_combat(punchbag(), &mut lines).unwrap();
        assert!(
            g.running,
            "`exit` is not compared at 1000:4c56 and must not Halt the game"
        );
        assert_eq!(lines.count(), 0, "both `exit`s and the `run` were read");
        assert!(g.last_enemy.is_some(), "the fight ended by fleeing");
        assert!(
            g.rng.take_log().is_empty(),
            "and `exit` reached no arm that draws either"
        );
    }

    /// The property the captured oracles rest on: a fight that types only
    /// the verbs the captures typed spends its draws at exactly the sites it
    /// spent them at before Task 18 -- none of the four new ones -- even
    /// when the player is carrying everything the new arms need.
    ///
    /// `data/combat_trace.json` is the real check (15 fights, 1900 draws);
    /// this is the same statement in a form that names the four sites, so a
    /// regression says which one leaked rather than only that the stream
    /// moved.
    #[test]
    fn an_ordinary_fight_never_touches_the_four_new_random_sites() {
        const NEW: [&str; 4] = ["1000:4db7", "1000:4e16", "1000:4ef5", "1000:4f18"];
        let mut blows = 0;
        for seed in 0..40u32 {
            let mut g = game_with_gopota();
            g.rng = Rng::new(seed);
            g.has_mobile = true;
            g.pistol = combat_dispatch::Pistol {
                owned: true,
                silencer: true,
                cartridges: 99,
            };
            g.rng.start_log();
            // Ten `k`s on a player who owns a pistol and could call the
            // gopota, but types neither `f` nor `v`.
            g.run_combat(punchbag(), &mut input(&["k"; 10])).unwrap();
            let log = g.rng.take_log();
            blows += log.iter().filter(|d| d.site == "1000:4460").count();
            for site in NEW {
                assert_eq!(
                    log.iter().filter(|d| d.site == site).count(),
                    0,
                    "seed {seed}: {site} fired without `v` or `f` being typed"
                );
            }
            assert!(
                !log.is_empty(),
                "seed {seed}: the fight drew nothing at all"
            );
        }
        assert!(
            blows > 0,
            "no blow was ever rolled -- the script did nothing"
        );
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

        g.run_combat(punchbag(), &mut input(&["run"])).unwrap();
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
        g.run_combat(punchbag(), &mut input(&["run"])).unwrap();
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
            g.run_combat(punchbag(), &mut input(&["run"])).unwrap();
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
        g.run_combat(punchbag(), &mut input(&["run"])).unwrap();
        assert!(!g.places.is_found(Location::Den), "class 5 skips 1000:4a8e");
        assert_eq!(g.player.level, 2, "but still pays the level");
    }

    /// `1000:411d` -- the rector showdown suppresses the spectators, and
    /// with them their two draws, while leaving the counter (and its
    /// `^7Начинают собираться зрители` at exactly five) alone.
    #[test]
    fn the_rector_showdown_has_no_spectators() {
        for (rector, want) in [(false, 6usize), (true, 0)] {
            let mut g = game();
            g.rector_showdown = rector;
            g.rng.start_log();
            // Ten prompts, the last of them the `run` that ends the fight --
            // without it the loop prompts an eleventh time before it sees the
            // input end. 1000:4135 fires from the fifth prompt onward, so six.
            let mut script = vec!["zzz"; 9];
            script.push("run");
            g.run_combat(punchbag(), &mut input(&script)).unwrap();
            assert_eq!(
                draws_at(&mut g, "1000:4135"),
                want,
                "rector_showdown = {rector}"
            );
        }
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
        g.run_combat(punchbag(), &mut lines).unwrap();
        assert_eq!(lines.count(), 0, "1000:490b re-prompts instead of leaving");
        assert_eq!(g.player.level, 5, "and the penalty never runs");

        // Death: the hospital's own gates are wide open and it still must
        // not fire.
        let mut g = game_with_gopota();
        g.rector_showdown = true;
        g.player.hp = 0;
        g.player.hpmax = 40;
        g.player.money = 100;
        g.run_combat(punchbag(), &mut no_input()).unwrap();
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
        g.run_combat(enemy(), &mut lines).unwrap();
        assert_eq!(g.player.joints, 1, "1000:4b4e dec [0x38c5]");
        assert_eq!(g.player.hp, 11, "the flat +10 heal");
        assert_eq!(g.buff_countdown, 3, "1000:4b52 stores 3, not 10");
        assert!(g.player.stoned);
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
        g.shop_turn(Location::Vet, "mar", &mut no_input()).unwrap(); // must not teleport
        assert_eq!(g.location, Location::Vet);
        g.shop_turn(Location::Vet, "w", &mut no_input()).unwrap();
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
        g.shop_turn(Location::Vet, "h", &mut no_input()).unwrap();
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
        g.shop_turn(Location::Vet, "r", &mut no_input()).unwrap();
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
    /// creation already makes at `1000:7220` / `1000:7227`.
    #[test]
    fn an_empty_rename_installs_the_default_name() {
        let mut g = game();
        g.player.name = "Вася".to_string();
        let mut lines = input(&[""]);
        g.rename(&mut lines).unwrap();
        assert_eq!(g.player.name, "Раз^6дол^4бай");

        let mut g = game();
        g.player.name = "Вася".to_string();
        let mut lines = input(&["Петя"]);
        g.rename(&mut lines).unwrap();
        assert_eq!(g.player.name, "Петя");
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
        assert_eq!(g.player.name, "   ");
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
        for (money, want) in [(1i32, 1i32), (2, 0)] {
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
        assert!(!g.wear_jacket_38b6, "1000:c0e0 must not have run");
        g.district = 2;
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 1000 - 25);
        assert!(g.wear_jacket_38b6, "1000:c0e0");
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
        for (money, want) in [(149i32, false), (150, true)] {
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
    fn dealers(money: i32) -> Game {
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
        for (money, want) in [(14i32, 0u16), (15, 1)] {
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
        for (money, want) in [(29i32, false), (30, true)] {
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

    /// `bmar` row 3, Офигенный косяк -- the only one of the nine that draws.
    /// There is no RNG setter (see [`crate::rng::Rng::state`]), so each of
    /// the four arms of the `Random(4)` at `1000:ca0c` is reached by seed
    /// search and its own numbers asserted.
    #[test]
    fn the_good_joint_rolls_one_of_four_stat_points() {
        let mut seen = [false; 4];
        let mut odd_case_checked = false;
        for seed in 0u32..256 {
            let mut g = Game::new(player(), Progress::new(), seed);
            g.location = Location::Dealers;
            g.mode = Mode::Shop(Location::Dealers);
            g.player.money = 100;
            let was = g.player.clone();
            g.rng.start_log();
            g.shop_turn(Location::Dealers, "3", &mut no_input())
                .unwrap();
            let log = g.rng.take_log();
            assert_eq!(log.len(), 1, "one draw per purchase, seed {seed}");
            assert_eq!(log[0].site, "1000:ca0c");
            assert_eq!(log[0].n, 4, "1000:ca08 `mov ax,0x4`");
            assert_eq!(g.player.money, 80, "20ae:0b3a = 20, debit 1000:c9eb");
            let roll = log[0].r as usize;
            seen[roll] = true;
            match roll {
                // 1000:ca11
                0 => {
                    assert_eq!(g.player.strength, was.strength + 1, "1000:ca16");
                    assert_eq!(g.player.dmg_max, was.dmg_max + 1, "1000:ca33");
                    // player() starts at 5, so the NEW Сила is 6 -- even, and
                    // 1000:ca45 runs.
                    assert_eq!(g.player.dmg_min, was.dmg_min + 1, "1000:ca45");
                    assert_eq!(g.player.hpmax, was.hpmax + 1, "1000:ca49");
                    assert_eq!(g.player.hp, was.hp + 1, "1000:ca4d");
                    // Same seed, one less Сила: the new value is 5, odd, and
                    // 1000:ca43 `jnz 0xca49` skips the dmg-min half while
                    // every other write still happens.
                    let mut h = Game::new(player(), Progress::new(), seed);
                    h.location = Location::Dealers;
                    h.mode = Mode::Shop(Location::Dealers);
                    h.player.money = 100;
                    h.player.strength = 4;
                    h.shop_turn(Location::Dealers, "3", &mut no_input())
                        .unwrap();
                    assert_eq!(h.player.strength, 5);
                    assert_eq!(h.player.dmg_min, was.dmg_min, "1000:ca45 is skipped");
                    assert_eq!(h.player.dmg_max, was.dmg_max + 1, "1000:ca33 is not");
                    odd_case_checked = true;
                }
                // 1000:ca53
                1 => {
                    assert_eq!(g.player.agility, was.agility + 1, "1000:ca58");
                    assert_eq!(g.player.strength, was.strength);
                    assert_eq!(g.player.hpmax, was.hpmax);
                }
                // 1000:ca77
                2 => {
                    assert_eq!(g.player.vitality, was.vitality + 1, "1000:ca7c");
                    assert_eq!(g.player.hpmax, was.hpmax + 5, "1000:ca99");
                    assert_eq!(g.player.hp, was.hp + 5, "1000:ca9e");
                }
                // 1000:caa5
                _ => {
                    assert_eq!(g.player.luck, was.luck + 1, "1000:caaa");
                    assert_eq!(g.player.hpmax, was.hpmax);
                }
            }
        }
        assert!(
            seen.iter().all(|s| *s),
            "all four rolls exercised: {seen:?}"
        );
        assert!(odd_case_checked, "the odd-Сила half of roll 0 was reached");
        // Repeatable -- no already-own test in the arm (1000:c9b5's span).
        let mut g = dealers(100);
        g.shop_turn(Location::Dealers, "3", &mut no_input())
            .unwrap();
        g.shop_turn(Location::Dealers, "3", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 60, "two purchases, 20 each");
        // Too poor: 1000:c9c8 is `jle`.
        let mut g = dealers(19);
        g.rng.start_log();
        g.shop_turn(Location::Dealers, "3", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 19);
        assert!(g.rng.take_log().is_empty(), "a refusal draws nothing");
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
        for (money, want) in [(9i32, false), (10, true)] {
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
        assert!(g.weapon_kastet_38ba, "1000:cb9d");
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
            g.weapon_dubinka_394b = club;
            g.weapon_nozhik_38c2 = knife;
            g.weapon_tesak_394c = cleaver;
            g.shop_turn(Location::Dealers, "5", &mut no_input())
                .unwrap();
            assert_eq!(
                g.weapon_kastet_38ba, want,
                "club {club} knife {knife} cleaver {cleaver}"
            );
        }
        // Too poor: 1000:cb80 is `jle`.
        for (money, want) in [(24i32, false), (25, true)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "5", &mut no_input())
                .unwrap();
            assert_eq!(g.weapon_kastet_38ba, want, "money {money}");
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
        assert!(g.weapon_dubinka_394b, "1000:cc56");
        assert_eq!(g.player.money, 10, "20ae:0b3d = 50, debit 1000:cc60");
        assert_eq!(g.player.dmg_min, min, "1000:cc69 skips 1000:cc6b");
        assert_eq!(g.player.dmg_max, max, "1000:cc69 skips 1000:cc70");

        // With the knuckles it is +2/+2 -- never the +4/+4 the loot arm has
        // at 1000:55e6.
        let mut g = dealers(60);
        g.weapon_kastet_38ba = true;
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
            g.weapon_nozhik_38c2 = knife;
            g.weapon_tesak_394c = cleaver;
            g.shop_turn(Location::Dealers, "6", &mut no_input())
                .unwrap();
            assert_eq!(
                g.weapon_dubinka_394b, want,
                "knife {knife} cleaver {cleaver}"
            );
        }
        // Too poor: 1000:cc39 is `jle`.
        for (money, want) in [(49i32, false), (50, true)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "6", &mut no_input())
                .unwrap();
            assert_eq!(g.weapon_dubinka_394b, want, "money {money}");
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
        assert_eq!(
            listed(4),
            ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
            "and lists everything once the district is high enough"
        );

        let mut g = dealers(1_000);
        assert_eq!(g.district, 1);
        g.shop_turn(Location::Dealers, "5", &mut no_input())
            .unwrap(); // gate district>1
        assert!(g.weapon_kastet_38ba, "1000:cb9d fires at district 1");
        g.shop_turn(Location::Dealers, "6", &mut no_input())
            .unwrap(); // gate district>2
        assert!(g.weapon_dubinka_394b, "1000:cc56 fires at district 1");
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

    fn market(money: i32) -> Game {
        let mut g = game();
        g.location = Location::Market;
        g.mode = Mode::Shop(Location::Market);
        g.player.money = money;
        g
    }

    /// `mar` row 1, Хотдог. The heal is `3 + Random(2)` -- `1000:bdb7`
    /// pushes the 2, `1000:bdbb` draws, `1000:bdc0 add ax,0x3` consumes it
    /// and `1000:bdc3 add [0x38ac],ax` applies it -- clamped back to hp max
    /// by `1000:bdce jle 0xbdd6` over `1000:bdd3 mov [0x38ac],ax`.
    #[test]
    fn the_market_hot_dog_heals_three_or_four_and_clamps_to_hp_max() {
        let mut g = market(10);
        g.player.hp = 1;
        g.rng.start_log();
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        let log = g.rng.take_log();
        assert_eq!(log.len(), 1, "1000:bdbb draws once");
        assert_eq!((log[0].site, log[0].n), ("1000:bdbb", 2));
        assert_eq!(g.player.hp, 1 + 3 + log[0].r, "1000:bdc0 / 1000:bdc3");
        assert!(matches!(g.player.hp, 4 | 5), "hp {}", g.player.hp);
        assert_eq!(g.player.money, 8, "1000:bdb3, price 2 at 20ae:0b2e");

        // The clamp: 19/20 heals to 20, never to 22 or 23.
        let mut g = market(10);
        g.player.hp = 19;
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        assert_eq!(g.player.hp, 20, "1000:bdd3");
        assert_eq!(g.player.hpmax, 20);

        // No already-own test: the row is repeatable and draws every time.
        let mut g = market(10);
        g.player.hp = 1;
        g.rng.start_log();
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        assert_eq!(g.rng.take_log().len(), 2, "1000:bdbb draws every purchase");
        assert_eq!(g.player.money, 6);
    }

    /// Row 1's three gates, each on its refusing side. None of them reaches
    /// the draw at `1000:bdbb`, which is what makes a refusal RNG-neutral.
    #[test]
    fn the_market_hot_dog_refuses_on_a_broken_jaw_full_health_and_no_money() {
        // 1000:bd5c `cmp byte [0x38b0],0x1` / 1000:bd61 `jnz 0xbd7f` -- the
        // branch jumps PAST the refusal at 1000:bd63, so a broken jaw
        // refuses on the fall-through.
        let mut g = market(10);
        g.player.hp = 1;
        g.player.broken_jaw = true;
        g.rng.start_log();
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 10, "1000:bd63 is a refusal, not a sale");
        assert_eq!(g.player.hp, 1);
        assert!(g.rng.take_log().is_empty());

        // 1000:bd82 `cmp ax,[0x38ae]` / 1000:bd86 `jnl 0xbdf1` -- hp is
        // already at max, which is what `player()` ships.
        let mut g = market(10);
        g.rng.start_log();
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 10, "1000:bdf1 is a refusal, not a sale");
        assert_eq!(g.player.hp, 20);
        assert!(g.rng.take_log().is_empty());

        // 1000:bd91 `jle` -- the refusal at 1000:bd93 is the fall-through.
        let mut g = market(1);
        g.player.hp = 1;
        g.rng.start_log();
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 1);
        assert_eq!(g.player.hp, 1);
        assert!(g.rng.take_log().is_empty());
    }

    /// `mar` row 2, Пиво. `1000:beb4 inc [0x38c3]` is the effect; the
    /// `Random(3)` at `1000:be51` picks one of three lines and changes no
    /// state at all -- and is drawn anyway, because a skipped draw
    /// desynchronises every later one.
    #[test]
    fn the_market_beer_counts_up_and_draws_a_die_that_changes_nothing() {
        let mut g = market(12);
        g.rng.start_log();
        g.shop_turn(Location::Market, "2", &mut no_input()).unwrap();
        let log = g.rng.take_log();
        assert_eq!(log.len(), 1, "1000:be51 draws even though nothing reads it");
        assert_eq!((log[0].site, log[0].n), ("1000:be51", 3));
        assert_eq!(g.player.beer_dl, 1, "1000:beb4");
        assert_eq!(g.player.money, 7, "1000:be49, price 5 at 20ae:0b2f");

        // Repeatable, and each purchase draws again.
        g.rng.start_log();
        g.shop_turn(Location::Market, "2", &mut no_input()).unwrap();
        assert_eq!(g.rng.take_log().len(), 1);
        assert_eq!(g.player.beer_dl, 2);
        assert_eq!(g.player.money, 2);

        // 1000:be27 `jle` refuses at 4, and the refusal rejoins at
        // 1000:be42 `jmp short 0xbeb8` -- PAST 1000:beb4, so no beer.
        let mut g = market(4);
        g.rng.start_log();
        g.shop_turn(Location::Market, "2", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 4);
        assert_eq!(g.player.beer_dl, 0, "a failed purchase adds no beer");
        assert!(g.rng.take_log().is_empty());
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
        for (money, want) in [(9i32, false), (10, true)] {
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
        assert!(g.wear_suit_abibas_38b4, "1000:bf80");
        assert_eq!(g.player.armor, 1, "1000:bfa7");
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert!(g.wear_suit_adidas_38b7, "1000:c183");
        assert_eq!(g.player.armor, 2, "1000:c1b1, the delta, not 1000:c1b7");
        assert_eq!(g.player.money, 100 - 15 - 30);

        // The adidas suit alone takes the full bonus...
        let mut g = market(100);
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert_eq!(g.player.armor, 2, "1000:c1b7");
        // ...and row 4's better-item gate 1000:bf51 then refuses, free.
        g.shop_turn(Location::Market, "4", &mut no_input()).unwrap();
        assert!(!g.wear_suit_abibas_38b4, "1000:bfc8 is a refusal");
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
        assert!(g.wear_boots_38b5, "1000:c029");
        assert_eq!(
            (g.player.dmg_min, g.player.dmg_max),
            (base_min + 1, base_max + 1),
            "1000:c050 / 1000:c054"
        );
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        assert!(g.wear_boots_pontovye_38b8, "1000:c222");
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
        assert!(!g.wear_boots_38b5, "1000:c075 is a refusal");
        assert_eq!(g.player.money, 70);
        // Row 8's already-own gate 1000:c1fa.
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 70, "1000:c266 is a refusal, not a sale");

        // Row 5's own already-own gate 1000:c001, and its `jle` at
        // 1000:c00c: 15 exactly buys, 14 does not.
        for (money, want) in [(14i32, false), (15, true)] {
            let mut g = market(money);
            g.shop_turn(Location::Market, "5", &mut no_input()).unwrap();
            assert_eq!(g.wear_boots_38b5, want, "money {money}");
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
        assert!(g.wear_jacket_38b6, "1000:c0e0");
        assert_eq!(g.player.armor, 2, "1000:c107");
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert!(g.wear_jacket_krutaya_38b9, "1000:c2ca");
        assert_eq!(g.player.armor, 4, "1000:c2f8, the delta, not 1000:c2ff");
        assert_eq!(g.player.money, 100 - 25 - 50);

        // The better jacket alone takes the full +4.
        let mut g = market(100);
        g.district = 4;
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert_eq!(g.player.armor, 4, "1000:c2ff");
        // Row 6's better-item gate 1000:c0b1 then refuses, free.
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert!(!g.wear_jacket_38b6, "1000:c129 is a refusal");
        assert_eq!(g.player.money, 50);
        // Row 9's already-own gate 1000:c2a2.
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 50, "1000:c306 is a refusal, not a sale");

        // Row 6's already-own gate 1000:c0b8, and its `jle` at 1000:c0c3.
        for (money, want) in [(24i32, false), (25, true)] {
            let mut g = market(money);
            g.district = 2;
            g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
            assert_eq!(g.wear_jacket_38b6, want, "money {money}");
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
        assert!(!g.wear_jacket_38b6, "1000:c08e is on the buy path");
        assert_eq!(g.player.money, 100);
        // Row 7 is NOT: 1000:c142 is reached with no district test.
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert!(g.wear_suit_adidas_38b7, "1000:c183 fires at district 1");
        assert_eq!(g.player.money, 70);
        assert_eq!(g.player.armor, 2, "1000:c1b7");
        // Rows 8 and 9 have matching menu and buy gates, so both stay shut.
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert_eq!(
            g.player.money, 70,
            "1000:c1de and 1000:c286 skip, in silence"
        );
        assert!(!g.wear_boots_pontovye_38b8);
        assert!(!g.wear_jacket_krutaya_38b9);
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

    /// The whole chain, end to end: buy the pistol at the dealers, then fire
    /// it in a fight. Before Task 18 neither half existed, and `f` at either
    /// prompt printed an invented refusal.
    #[test]
    fn a_pistol_bought_at_the_dealers_can_be_fired_in_a_fight() {
        let mut g = game();
        g.location = Location::Dealers;
        g.mode = Mode::Shop(Location::Dealers);
        g.district = 4;
        g.player.money = 1_000;
        g.player.agility = 50; // beats every Random(0x32)
        g.flag_3693 = true; // 1000:4ebc, the shooting is permitted here
        g.shop_turn(Location::Dealers, "7", &mut no_input())
            .unwrap();
        assert_eq!(g.pistol.cartridges, 3);

        g.mode = Mode::Street;
        g.rng.start_log();
        g.run_combat(punchbag(), &mut input(&["f", "run"])).unwrap();
        assert_eq!(g.pistol.cartridges, 2, "1000:4eed spent one");
        let left = g.last_enemy.as_ref().unwrap().hp;
        assert!(
            (500 - 29..=500 - 20).contains(&left),
            "the shot did 20..=29: enemy hp {left}"
        );
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
            (|g| g.charm_krestik_38bd = true, "^1Крестик(Удача +2) "),
            (|g| g.charm_ring_38be = true, "^1Кольцо \"Гс\"(Удача +1) "),
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
            (|g| g.wear_boots_38b5 = true, "^1Бутсы(+1) "),
            (
                |g| g.wear_boots_pontovye_38b8 = true,
                "^1Понтовые бутсы(Урон+2) ",
            ),
            (|g| g.weapon_kastet_38ba = true, "^1Кастет(+2) "),
            (|g| g.weapon_dubinka_394b = true, "^1Дубинка(+4)  "),
            (|g| g.weapon_nozhik_38c2 = true, "^1Нож(+6) "),
            (|g| g.weapon_tesak_394c = true, "^1Тесак(Урон+9) "),
            (|g| g.tooth_guard = true, "^1Зубная защита  "),
            (|g| g.buff_countdown = 3, "^6Обдолбаный  "),
            (
                |g| {
                    g.player.armor = 2;
                    g.wear_suit_abibas_38b4 = true;
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
            (|g| g.wear_suit_adidas_38b7 = true, "^1Костюм Adidas(+2) "),
            (|g| g.wear_jacket_38b6 = true, "^1Кожанка(+2) "),
            (
                |g| g.wear_jacket_krutaya_38b9 = true,
                "^1Крутая кожанка(+4) ",
            ),
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

    #[test]
    fn sell_junk_and_sell_items_are_the_dealers_own_keys() {
        let mut g = game();
        g.location = Location::Dealers;
        g.mode = Mode::Shop(Location::Dealers);
        // Neither has a backing field yet, so the only observable effect is
        // that the keys are routed at all: they must not leave the shop.
        g.shop_turn(Location::Dealers, "x", &mut no_input())
            .unwrap();
        assert_eq!(g.mode, Mode::Shop(Location::Dealers));
        g.shop_turn(Location::Dealers, "wes", &mut no_input())
            .unwrap();
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

    /// The seed-0 `RandSeed` chain out of `data/rng_vectors.json` -- the
    /// 8086-interpreter oracle, not this port. See
    /// `crate::combat::tests::ground_truth_states`, which reads the same
    /// array for the same reason.
    fn ground_truth_states() -> Vec<u32> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/rng_vectors.json");
        let bytes = std::fs::read(path).expect("read data/rng_vectors.json");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        let block = &v["seeds"][0];
        assert_eq!(block["seed"].as_u64(), Some(0));
        block["next_u32"]
            .as_array()
            .expect("next_u32")
            .iter()
            .map(|x| x.as_u64().expect("u32") as u32)
            .collect()
    }

    fn random_of(state: u32, n: u16) -> u16 {
        ((state as u64 * n as u64) >> 32) as u16
    }

    /// The зубная защита's two arms, driven through a whole round.
    ///
    /// `combat::tests` pins the DRAW; this pins what the round does with it.
    /// `1000:4820` (file `0x4BB1`) sets `[0x38b0]` and `1000:4827` (file
    /// `0x4BE5`) does not, so the two arms differ in guest STATE and not
    /// only in which line is printed -- which is what makes them assertable
    /// here at all.
    ///
    /// The round is arranged so its draw stream is one player miss followed
    /// by one enemy swing that hits, crits and breaks a jaw: the player's
    /// agility 0 caps his accuracy at `4 * 5 = 20` (`1000:446a`) and both
    /// chain indices below open above that, while the enemy's agility 14
    /// gives him exactly one blow at the 90 cap and his luck 300 decides
    /// every crit and break comparison by itself.
    #[test]
    fn the_zubnaya_zashchita_arms_differ_in_the_players_jaw_not_only_in_the_line() {
        let st = ground_truth_states();
        // chain index 3 -> the guard's Random(4) is 0, chain index 2 -> it
        // is not; both are computed below rather than written down.
        for k in [3usize, 2] {
            let guard_draw = random_of(st[k + 7], 4);
            let mut g = game();
            g.rng = Rng::new(st[k - 1]);
            g.tooth_guard = true;
            g.player.agility = 0;
            g.player.hp = 50;
            g.player.hpmax = 50;
            let mut enemy = Fighter {
                agility: 14,
                luck: 300,
                dmg_min: 1,
                dmg_max: 3,
                hp: 50,
                hpmax: 50,
                ..Fighter::default()
            };
            g.rng.start_log();
            g.combat_round(&mut enemy);
            let log = g.rng.take_log();
            let sites: Vec<&str> = log.iter().map(|d| d.site).collect();
            assert_eq!(
                sites,
                [
                    "1000:4460", // the player's miss: one draw, then his half ends
                    "1000:4683",
                    "1000:46ba",
                    "1000:46db",
                    "1000:4706",
                    "1000:4794",
                    "1000:47be",
                    "1000:47fe",
                ],
                "chain index {k}: draw shape"
            );
            assert_eq!(
                g.player.broken_jaw,
                guard_draw == 0,
                "chain index {k}: Random(4) = {guard_draw}; 1000:4803 `or ax,ax` / \
                 `jnz 0x4827` means only 0 reaches the setter at 1000:4820"
            );
        }
        assert_ne!(
            random_of(st[10], 4) == 0,
            random_of(st[9], 4) == 0,
            "the two chain indices must land on DIFFERENT arms, or the loop \
             above is one case written twice"
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

    /// Runs one whole walk on `seed` and reports what the decline branch's
    /// `Random(2)` at `1000:b725` returned, or `None` when the turn produced
    /// no fight encounter at all.
    ///
    /// **This observes the draw log rather than predicting it.** An earlier
    /// version hand-replayed what it believed `walk`'s draws to be, and its
    /// own doc admitted the flaw: "if `walk` gained, lost or reordered a
    /// `Random` call, this helper would drift with it and stay green."
    /// Task 11c added the eleven preamble draws that were missing, and the
    /// helper duly broke -- which is the drift, not a conflict with the
    /// finding the tests below assert. Reading `1000:b725` out of
    /// [`crate::rng::Rng`]'s log cannot drift: it names the call site.
    ///
    /// What it still cannot do is tell whether the *sequence* is right; only
    /// `tests/wander_sequence.rs`, replaying captured runs of the original,
    /// settles that.
    fn decline_roll_for(seed: u32) -> Option<u16> {
        let mut g = Game::new(player(), Progress::new(), seed);
        g.rng.start_log();
        g.walk(&mut input(&["n"])).unwrap();
        g.rng
            .take_log()
            .iter()
            .find(|d| d.site == "1000:b725")
            .map(|d| d.r)
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

    /// The first seed whose walk reaches the girl event and whose
    /// `Random(2)` at `1000:b54e` returns `want`. Observed from the draw
    /// log, for the reason [`decline_roll_for`] gives.
    fn girl_seed_with_roll(want: u16) -> u32 {
        (1u32..40_000)
            .find(|&seed| {
                let mut g = Game::new(player(), Progress::new(), seed);
                g.rng.start_log();
                g.walk(&mut input(&["y"])).unwrap();
                g.rng
                    .take_log()
                    .iter()
                    .any(|d| d.site == "1000:b54e" && d.r == want)
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
    /// whose tail-call at `1000:0ac0` reaches the RTL's `mov ah,0x4c` /
    /// `int 0x21` at file `0x1123C` -- see [`Game::run_combat`]), rather
    /// than walking on as a 0-HP corpse.
    #[test]
    fn player_death_stops_the_loop() {
        let mut g = game();
        g.player.hp = 0;
        let enemy = player();
        g.run_combat(enemy, &mut no_input()).unwrap();
        assert!(!g.running, "death must end the game");
    }

    /// Which [`IMM_ROWS`] rows [`Game::imm_row_visible`] lets through, for a
    /// given district / level / armour.
    fn visible(district: u8, level: u16, armor: u16) -> Vec<(&'static str, &'static str)> {
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
    /// (`1000:e576`, `1000:e57d`..`1000:e58d`). `abs` is `20ae:3e34`, which
    /// this port carries as plain armour -- see [`Game::imm_row_visible`].
    #[test]
    fn the_gyms_abs_row_needs_a_third_district_and_room_to_train() {
        assert!(!visible(2, 0, 0).contains(&("trn", "5")));
        assert!(visible(3, 0, 5).contains(&("trn", "5")));
        assert!(!visible(3, 0, 6).contains(&("trn", "5")));
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
        g.run_combat(enemy, &mut no_input()).unwrap();
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
        let pontovost_street: i32 = 0;
        // ax = (level - (district-1)*10) * 2 + pontovost_street; solved for
        // ax == 0x28 exactly (the boundary), then one level below it.
        let boundary_level = ((0x28 - pontovost_street) / 2 + (district - 1) * 10) as u16;

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
        let row = |cred: i32, credit: u8| {
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
        let lines_for = |errand2: bool, cred: i32| {
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

    /// Menu lines 6 and 13 share one byte, `1000:d8c8`/`1000:da35`, and the
    /// `hp` ARM's own gate `1000:dbf3` reads the same one. Pinned together
    /// so a port that offered the row without arming the arm (or the
    /// reverse) goes red.
    #[test]
    fn the_den_errand_one_row_and_the_hp_arm_share_their_byte() {
        for pending in [false, true] {
            let mut g = game();
            g.district = 1;
            g.den_errand_1_pending = pending;
            let menu = term::capture::lines(|| g.print_den_menu());
            let has = |t: &str| menu.iter().any(|l| l == t);
            assert_eq!(has("^6На одного пацана наехал какой-то урод"), pending);
            assert_eq!(
                has("Напиши ^6hp^7 чтобы отпинать мудака который наезжал на пацана"),
                pending
            );
            g.rng.start_log();
            let out = term::capture::lines(|| {
                g.shop_turn(Location::Den, "hp", &mut input(&["run"]))
                    .unwrap()
            });
            assert_eq!(
                !out.is_empty(),
                pending,
                "the arm must fire exactly when the row is offered"
            );
            assert_eq!(!g.rng.take_log().is_empty(), pending);
        }
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

    /// `p` -- `1000:db22`..`1000:db77`. Gate `1000:db38`, effects
    /// `1000:db3a` and `1000:db3e`, both strings.
    #[test]
    fn den_p_spends_a_half_litre_and_raises_the_street_cred_by_five() {
        let mut g = game();
        g.player.beer_dl = 2;
        g.pontovost_street = 7;
        g.rng.start_log();
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "p", &mut no_input()).unwrap());
        assert_eq!(
            out,
            vec!["^2Ты угостил пацанов пивом. Понтовость улутшилась на 5."]
        );
        assert_eq!(g.player.beer_dl, 1, "1000:db3a `dec [0x38c3]`");
        assert_eq!(g.pontovost_street, 12, "1000:db3e `add word [0x38cb],0x5`");
        assert!(
            g.rng.take_log().is_empty(),
            "1000:db22..1000:db77 holds no `call 0f78:114b`"
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
        let refusal = |credit: u8, cred: i32| {
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

    /// `s` -- `1000:dc63`..`1000:dcba`. The first line always, the second
    /// on `district*10 + 10 <= [0x38cb]` (`1000:dc98`/`1000:dc9f`), and the
    /// arm writes nothing at all.
    #[test]
    fn den_s_prints_the_cred_and_adds_the_second_line_at_the_threshold() {
        let ask = |district: u8, cred: i32| {
            let mut g = game();
            g.district = district;
            g.pontovost_street = cred;
            g.player.money = 33;
            g.player.beer_dl = 4;
            g.den_loan_credit = 2;
            g.rng.start_log();
            let out =
                term::capture::lines(|| g.shop_turn(Location::Den, "s", &mut no_input()).unwrap());
            // The measured no_effect_claim over 1000:dc63..1000:dcba: the
            // absolute-write sweep finds zero stores in the span.
            assert_eq!(g.pontovost_street, cred);
            assert_eq!(g.player.money, 33);
            assert_eq!(g.player.beer_dl, 4);
            assert_eq!(g.den_loan_credit, 2);
            assert!(g.rng.take_log().is_empty());
            out
        };
        // district 1 -> the threshold is 20.
        assert_eq!(
            ask(1, 20),
            vec![
                "^4Твоя понтовость сейчас = 20.",
                "^0Да если чё мы за тебя впрягаемся.",
            ]
        );
        assert_eq!(ask(1, 19), vec!["^4Твоя понтовость сейчас = 19."]);
        // district 3 -> 40, so the same cred that passed at district 1 fails.
        assert_eq!(ask(3, 20), vec!["^4Твоя понтовость сейчас = 20."]);
        assert_eq!(
            ask(3, 40),
            vec![
                "^4Твоя понтовость сейчас = 40.",
                "^0Да если чё мы за тебя впрягаемся.",
            ]
        );
    }

    /// `hp` -- `1000:dbf3`'s gate stands IN FRONT of `1000:dc04`'s key
    /// compare, so with no errand pending the token is never compared: no
    /// output, no draw, no state change.
    #[test]
    fn den_hp_is_not_even_compared_without_an_errand() {
        let mut g = game();
        g.den_errand_1_pending = false;
        g.rng.start_log();
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "hp", &mut no_input()).unwrap());
        assert!(
            out.is_empty(),
            "1000:dbf8 falls through in silence: {out:?}"
        );
        assert!(g.rng.take_log().is_empty(), "no opponent may be rolled");
        assert!(!g.fight_accepted_3b72, "1000:dc11 must not run");
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
        assert!(g.fight_accepted_3b72, "1000:dc11 `mov byte [0x3b72],1`");
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

    /// `d` -- both gates at `1000:dd4b` and `1000:dd55` are SILENT, and
    /// neither spends a draw or consumes the errand.
    #[test]
    fn den_d_refuses_in_silence_below_a_hundred_cred_or_without_the_errand() {
        for (cred, errand) in [(99, true), (100, false), (0, false)] {
            let mut g = game();
            g.district = 1;
            g.pontovost_street = cred;
            g.den_errand_2_pending = errand;
            g.player.money = 5;
            g.rng.start_log();
            let out =
                term::capture::lines(|| g.shop_turn(Location::Den, "d", &mut no_input()).unwrap());
            assert!(out.is_empty(), "cred {cred}, errand {errand}: {out:?}");
            assert!(g.rng.take_log().is_empty(), "no draw before the gates pass");
            assert_eq!(g.den_errand_2_pending, errand, "1000:dec8 is not reached");
            assert_eq!(g.player.money, 5);
        }
    }

    /// `d`'s haul path: `1000:dda8`'s compare goes to `1000:de36` when luck
    /// wins. Three draws, at `1000:dd97`, `1000:de5a` and `1000:de7c`, with
    /// the `n` each site pushes; money and хлам each gain
    /// `district*10 + Random(district*10)`; the xp line prints `district*12`
    /// and `1000:debe` credits the same number.
    ///
    /// Luck is pinned at `0x7fff` so `Longint(luck) < Longint(Random(15))`
    /// is false for every possible roll -- the branch is selected by the
    /// predicate under test, not by a lucky seed.
    #[test]
    fn den_d_hauls_when_luck_wins_the_first_roll() {
        let mut g = game();
        g.district = 1;
        g.player.luck = 0x7fff;
        g.pontovost_street = 100;
        g.den_errand_2_pending = true;
        g.player.money = 0;
        g.player.junk = 0;
        let level_before = g.player.level;
        let threshold_before = g.progress.threshold;
        g.rng.start_log();
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "d", &mut no_input()).unwrap());
        let full = g.rng.take_log();
        let log: Vec<_> = full
            .iter()
            .filter(|d| d.site.starts_with("1000:dd") || d.site.starts_with("1000:de"))
            .cloned()
            .collect();
        assert_eq!(
            log.iter().map(|d| (d.site, d.n)).collect::<Vec<_>>(),
            vec![("1000:dd97", 15), ("1000:de5a", 10), ("1000:de7c", 10)],
            "three draws in range, in order, with the `n` each site pushes"
        );
        assert_eq!(
            out,
            vec![
                "^0Давай быстрее..".to_string(),
                "^2Ты пришел воровать деньги".to_string(),
                "^2Ты наваровал денег".to_string(),
                "^6Ты получаешь 12 качков опыта".to_string(),
            ]
        );
        assert_eq!(
            g.player.money,
            10 + i32::from(log[1].r),
            "1000:de6d: district*10 + Random(district*10)"
        );
        assert_eq!(
            g.player.junk,
            10 + log[2].r,
            "1000:de8f: the same shape for хлам"
        );
        // 1000:debe credits district*12 = 12; 1000:dec5's FUN_1000_2526(0)
        // then drains it against the 10-point first threshold, which is why
        // the level rises and 2 xp is left over -- and why `full` carries
        // two extra draws at `1000:25fe`, the per-level stat rolls, that
        // `log` filters out.
        assert_eq!(g.player.level, level_before + 1, "1000:dec5 levelled up");
        assert_eq!(g.progress.xp, 12 - threshold_before);
        assert_eq!(g.progress.threshold, threshold_before + 10);
        assert!(
            full.len() > log.len(),
            "the level-up spends its own draws at 1000:25fe"
        );
        assert!(!g.den_errand_2_pending, "1000:dec8");
    }

    /// `d` at district 3: every `n` and every award is rebuilt from
    /// `[0x3692]`, so they all move together. Without this the district
    /// multipliers could all be hard-coded to 1 and the test above would
    /// still pass.
    #[test]
    fn den_d_scales_every_draw_and_every_award_with_the_district() {
        let mut g = game();
        g.district = 3;
        g.player.luck = 0x7fff;
        g.pontovost_street = 100;
        g.den_errand_2_pending = true;
        g.player.money = 0;
        g.player.junk = 0;
        g.rng.start_log();
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "d", &mut no_input()).unwrap());
        let log: Vec<_> = g
            .rng
            .take_log()
            .into_iter()
            .filter(|d| d.site.starts_with("1000:dd") || d.site.starts_with("1000:de"))
            .collect();
        assert_eq!(
            log.iter().map(|d| (d.site, d.n)).collect::<Vec<_>>(),
            vec![("1000:dd97", 45), ("1000:de5a", 30), ("1000:de7c", 30)]
        );
        assert!(out.contains(&"^6Ты получаешь 36 качков опыта".to_string()));
        assert_eq!(g.player.money, 30 + i32::from(log[1].r));
        assert_eq!(g.player.junk, 30 + log[2].r);
    }

    /// `d`'s "slipped away" path: luck loses `1000:dda8`'s compare and wins
    /// `1000:dde9`'s. Seed 6 is chosen because its first two `Random(15)`
    /// draws are 2 and 0, which with `luck == 0` is exactly
    /// `0 < 2` then `not (0 < 0)`. Two draws in range, no fight, no money.
    #[test]
    fn den_d_slips_away_when_luck_loses_once_and_wins_once() {
        let mut g = Game::new(player(), Progress::new(), 6);
        g.district = 1;
        g.player.luck = 0;
        g.pontovost_street = 100;
        g.den_errand_2_pending = true;
        g.player.money = 0;
        g.rng.start_log();
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "d", &mut no_input()).unwrap());
        let log = g.rng.take_log();
        assert_eq!(
            log.iter().map(|d| (d.site, d.n, d.r)).collect::<Vec<_>>(),
            vec![("1000:dd97", 15, 2), ("1000:ddda", 15, 0)],
            "two draws in range, and no third: no opponent was rolled"
        );
        assert_eq!(
            out,
            vec![
                "^0Давай быстрее..",
                "^2Ты пришел воровать деньги",
                "^4Шухер менты!",
                "^2Ты смылся от ментов.",
            ]
        );
        assert_eq!(g.player.money, 0, "nothing is stolen on this path");
        assert!(
            !g.den_errand_2_pending,
            "1000:dec8 runs on the cop paths too"
        );
    }

    /// `d`'s caught path: luck loses BOTH compares. Seed 3's first two
    /// `Random(15)` draws are 1 and 7. `1000:ddf6` rolls with
    /// `param_1 = 2`, which `1000:0dc0` forces to class 8 -- the `Мент` --
    /// and `1000:ddff` prints after the fight returns.
    #[test]
    fn den_d_fights_a_cop_when_luck_loses_twice() {
        let mut g = Game::new(player(), Progress::new(), 3);
        g.district = 1;
        g.player.luck = 0;
        g.pontovost_street = 100;
        g.den_errand_2_pending = true;
        g.rng.start_log();
        let out = term::capture::lines(|| {
            g.shop_turn(Location::Den, "d", &mut input(&["run"]))
                .unwrap()
        });
        let in_range: Vec<_> = g
            .rng
            .take_log()
            .into_iter()
            .filter(|d| d.site == "1000:dd97" || d.site == "1000:ddda")
            .map(|d| (d.site, d.n, d.r))
            .collect();
        assert_eq!(
            in_range,
            vec![("1000:dd97", 15, 1), ("1000:ddda", 15, 7)],
            "both luck rolls lost"
        );
        assert_eq!(
            g.last_enemy.as_ref().map(|e| e.class),
            Some(8),
            "1000:ddf6's param_1 == 2 forces the Мент"
        );
        assert_eq!(out.first().map(String::as_str), Some("^0Давай быстрее.."));
        assert!(
            out.contains(&"^4Шухер менты!".to_string()),
            "1000:ddb6: {out:?}"
        );
        assert_eq!(
            out.last().map(String::as_str),
            Some("^6Пора валить!"),
            "1000:ddff prints AFTER the fight: {out:?}"
        );
        assert!(!g.den_errand_2_pending, "1000:dec8");
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
        g.run_combat(dead_enemy, &mut no_input()).unwrap();
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
}
