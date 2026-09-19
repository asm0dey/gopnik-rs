//! The game's opening and its two other straight-line text dumps --
//! `docs/re/port-gaps.md` rows 1, 6, 8, 10, 15 and 16.
//!
//! Six rows, all of them text the port printed nothing of before:
//!
//! | block | original | what |
//! |---|---|---|
//! | [`SPLASH`] | `FUN_1000_02c2`, `1000:02c2`..`04be` | the title screen, called once from `1000:6a5a` |
//! | [`BACKSTORY`] | `1000:6de6`..`6f2b` | the university cold open, before the class menu |
//! | [`START_ARRIVAL`] / [`TUTORIAL`] | `1000:7262`..`7347`, `7369`..`73bb` | the district line at start-up, and district 1's three-line crib |
//! | [`ADVANCE_ARRIVAL`] | `1000:ad12`..`adbf` | the same district line when play promotes you |
//! | [`QUIT_TAIL`] | `1000:ee04`..`ee8b` | what `e` / `exit` prints on the way out |
//! | [`HELP_PLAIN`] / [`HELP_FRAGMENTS`] | `1000:5f64`..`633c` | the whole `help` verb |
//!
//! ## The district lines exist twice in the image, so they exist twice here
//!
//! [`START_ARRIVAL`]'s districts 2/3/4 and all of [`ADVANCE_ARRIVAL`] read
//! the same on screen and are **separate string copies** -- CS `0x6849`,
//! `0x6872`, `0x6891`, `0x68b8`, `0x68e0`, `0x68fb` against CS `0x8346`,
//! `0x836f`, `0x838e`, `0x83b5`, `0x83dd`, `0x83f8`. Folding them into one
//! constant would make `tools/difftest.py` compare one copy twice and stop
//! being able to see a difference between them, so both are transcribed.
//! This is the same decision [`crate::game::Game::announce_district`] and
//! [`crate::game::Game::enter_district_5`] already make for the district-5
//! line (CS `0x6925` vs `0x9CF2`).
//!
//! ## Screen control is dropped, as everywhere else in this port
//!
//! `1000:04a2` / `1000:04af` are `TextColor(0)` / `TextColor(15)` around the
//! splash's `ReadKey`, and `1000:04b7` is the `Crt` `ClrScr`
//! (`FUN_1f16_01cc`, the same call `1000:6a1c` opens `FUN_1000_6a0d` with).
//! `crate::term` has no persistent attribute and no screen, so all three go
//! the way `crate::ending`'s `TextColor` pair and `Delay`/`ClrScr` did. The
//! `ReadKey` between them is kept.

use std::io;

use crate::term;

/// What one of these blocks does in the gaps *between* its literal lines.
///
/// Each entry is `(index, events)`: the events the original emits after line
/// `index - 1` and before line `index`, in address order, with `index ==
/// len()` meaning "after the last line, before the block ends". `'B'` is a
/// bare `WriteLn` (`1000:02d1`'s `0f78:05dd` + `0f78:0291` pair, which
/// writes nothing and ends the line) and `'K'` is a `ReadKey`
/// (`0f16:031a`). Gaps with no events are **omitted**, so an empty table
/// means a block that runs straight through.
///
/// `tools/difftest.py` rebuilds exactly this from `orig/g.exe` by scanning
/// for those two instruction shapes between the literal sites it already
/// found, which is why the blank-line counts and the `ReadKey` placement
/// below are compared rather than eyeballed.
pub type Gaps = &'static [(usize, &'static str)];

/// The blocks that run straight through: no blank line and no `ReadKey`
/// anywhere inside them. Named rather than written as `&[]` at each use so
/// the claim is one object `tools/difftest.py` can contradict.
pub const NO_GAPS: Gaps = &[];

/// `FUN_1000_02c2`'s eleven literals, in address order -- eight banner rows
/// (CS `0x0000`, `0x40`, `0x80`, `0xc0`, `0x100`, `0x140`, `0x180`,
/// `0x1c0`, one colour digit each), the version line (CS `0x200`), the
/// prompt (CS `0x23e`) and the credit (CS `0x274`).
///
/// The blank `WriteLn`s between them are [`splash`]'s business, not the
/// array's: they carry no literal.
pub const SPLASH: [&str; 11] = [
    // 1000:0317 cs 0x0000
    "                  ^0┌──── ┌────┐ ┌────┐  │    │  │    │  │    /",
    // 1000:0330 cs 0x0040
    "                  ^1│     │    │ │    │  │    │  │    │  │   / ",
    // 1000:0349 cs 0x0080
    "                  ^2│     │    │ │    │  │    │  │    │  │  /  ",
    // 1000:0362 cs 0x00c0
    "                  ^3│     │    │ │    │  ├────┤  │   /│  │_/   ",
    // 1000:037b cs 0x0100
    "                  ^4│     │    │ │    │  │    │  │  / │  │ \\   ",
    // 1000:0394 cs 0x0140
    "                  ^5│     │    │ │    │  │    │  │ /  │  │  \\  ",
    // 1000:03ad cs 0x0180
    "                  ^6│     │    │ │    │  │    │  │/   │  │   \\ ",
    // 1000:03c6 cs 0x01c0
    "                  ^7│     └────┘ │    │  │    │  │    │  │    \\",
    // 1000:03ee cs 0x0200
    "                                                ^0Версия 1.02",
    // 1000:0434 cs 0x023e
    "                          ^6Нажми какую-нибудь кнопку",
    // 1000:0489 cs 0x0274
    "     ^02003 year, June,Sept                                         ^2by V.P.",
];

/// What the splash does in the gaps between its literals -- `1000:02cc`,
/// `02db`, `02ea`, `02f9`, `0308` before the first banner row, `1000:03df`
/// before the version line, `1000:0407`/`0416`/`0425` before the prompt,
/// `1000:044d`/`045c`/`046b`/`047a` before the credit, and `1000:04aa`'s
/// `ReadKey` after it. See [`Gaps`].
pub const SPLASH_GAPS: Gaps = &[(0, "BBBBB"), (8, "B"), (9, "BBB"), (10, "BBBB"), (11, "K")];

/// `1000:6de6`..`6f2b`, the new-character path's cold open. Eleven lines,
/// printed before the class menu `crate::game`'s caller runs next.
pub const BACKSTORY: [&str; 11] = [
    // 1000:6de6 cs 0x64b1
    "Год 2xxx от Р.Х.",
    // 1000:6e04 cs 0x64c2
    "Последний день ты пришел в универ",
    // 1000:6e1d cs 0x64e4
    "Ты по-страшному косил и забивал",
    // 1000:6e36 cs 0x6504
    "Ты ещё мог сдать все задания, которые ты взял у друзей",
    // 1000:6e4f cs 0x653b
    "Но тут...",
    // 1000:6e6d cs 0x6545
    "^6Ректор: Ах ты урод, чёртов забивала. Вали из универа!",
    // 1000:6e8b cs 0x657d
    "^2Ты: А типа чё?",
    // 1000:6ea9 cs 0x658e
    "^6Ректор: Ты отчислен мудак!!! Как ты был лохом так и останешься.",
    // 1000:6ec7 cs 0x65d0
    "^4Это слышали все и ты из пацана превратился в опушенного.",
    // 1000:6ef4 cs 0x660b
    "Ты неможешь стерпеть такой наезд, однако ректор офигительно крутой.",
    // 1000:6f0d cs 0x664f
    "Ты решил доказать свою крутизну всему миру(в твоем понимании - Городу).",
];

/// The backstory's seven `ReadKey`s (`1000:6dff`, `6e68`, `6e86`, `6ea4`,
/// `6ec2`, `6ee0`, `6f26`) and its one bare `WriteLn` (`1000:6ee5`), which
/// falls in the same gap as the sixth `ReadKey` and AFTER it -- hence the
/// `"KB"`, not `"BK"`. See [`Gaps`].
pub const BACKSTORY_GAPS: Gaps = &[
    (1, "K"),
    (5, "K"),
    (6, "K"),
    (7, "K"),
    (8, "K"),
    (9, "KB"),
    (11, "K"),
];

/// `1000:7262`..`7347` -- the district line printed once, on the way into
/// the game, for districts 1 to 4. Two lines per district, so district `d`
/// is `[(d - 1) * 2]` and the entry after it.
///
/// District 5's own arm (`1000:7347`) is not here: its line is written out
/// in [`crate::game::Game::announce_district`] (CS `0x6925`, a separate copy
/// of the string [`crate::game::Game::enter_district_5`] prints) and its
/// `[0x3c83]` store is [`crate::game::Game::apply_class_bonus`].
pub const START_ARRIVAL: [&str; 8] = [
    // 1000:7269 cs 0x67f6
    "^1Ты стоишь у дверей университета.",
    // 1000:7282 cs 0x6819
    "^1Отсюда ты начнешь свой нелёгкий путь гопника.",
    // 1000:72a2 cs 0x6849
    "^1Ты сел на автобус и попёрся на шлюз...",
    // 1000:72bb cs 0x6872
    "^1Там бродит шлюзовская шпана.",
    // 1000:72db cs 0x6891
    "^1На маршрутке ты доехал до ОбьГЭСа...",
    // 1000:72f4 cs 0x68b8
    "^1Здесь бродит уже более крутая гопота.",
    // 1000:7313 cs 0x68e0
    "^1Ты приехал в Ельцовку...",
    // 1000:732c cs 0x68fb
    "^1Ото всюду доносятся крики запинываемых.",
];

/// `1000:7369`..`73bb` -- three more lines, on a district-1 start only.
/// `1000:7369 cmp byte [0x3692],1` is the gate and it skips nothing else.
pub const TUTORIAL: [&str; 3] = [
    // 1000:7370 cs 0x6949
    "Доказать свою крутизну ты можешь, отпинывая разных мудаков.",
    // 1000:7389 cs 0x6985
    "Тебе придётся поработать над сабой, чтобы стать крутым.",
    // 1000:73a2 cs 0x69bd
    "Введи ^6i^7 чтобы посмотреть команды, введи ^6help^7 что бы узнать чё за батва.",
];

/// `1000:ad12`..`adbf` -- the district line again, this time on the turn
/// play promotes you. Districts 2, 3 and 4 only (`1000:ad15`, `ad4e`,
/// `ad87`), so district `d` is `[(d - 2) * 2]` and the entry after it;
/// district 1 is unreachable here (the promotion increments first) and
/// district 5's arm at `1000:adbf` is
/// [`crate::game::Game::enter_district_5`].
///
/// A **separate string copy** from [`START_ARRIVAL`]'s districts 2/3/4 --
/// see the module doc.
pub const ADVANCE_ARRIVAL: [&str; 6] = [
    // 1000:ad19 cs 0x8346
    "^1Ты сел на автобус и попёрся на шлюз...",
    // 1000:ad32 cs 0x836f
    "^1Там бродит шлюзовская шпана.",
    // 1000:ad52 cs 0x838e
    "^1На маршрутке ты доехал до ОбьГЭСа...",
    // 1000:ad6b cs 0x83b5
    "^1Здесь бродит уже более крутая гопота.",
    // 1000:ad8b cs 0x83dd
    "^1Ты приехал в Ельцовку...",
    // 1000:ada4 cs 0x83f8
    "^1Ото всюду доносятся крики запинываемых.",
];

/// `1000:ee04`..`ee8b` -- what `e` / `exit` at the STREET prompt prints
/// before the process ends. Two lines, then `1000:ee36 call 0x1a03` (the
/// full character sheet) and `1000:ee39`'s `ReadKey`.
///
/// `docs/re/gaps.md` used to record this as "two real strings, not wired
/// up"; the sheet call and the `ReadKey` are the rest of it.
///
/// The `ReadKey` is [`QUIT_GAPS`]; what that table cannot say is that
/// `1000:ee36`'s character-sheet call sits between the second line and it,
/// because a sheet call is neither of the two shapes a gap records.
pub const QUIT_TAIL: [&str; 2] = [
    // 1000:ee04 cs 0xab23
    "^6Блин не быть тебе нормальным пацаном",
    // 1000:ee1d cs 0xab4a
    "^1А результат:",
];

/// `1000:ee39` -- the one `ReadKey`, after the character sheet.
pub const QUIT_GAPS: Gaps = &[(2, "K")];

/// `help`'s thirty un-composed lines, in address order. The two composed
/// lines and the two `#`-filled ones are [`HELP_FRAGMENTS`] and
/// [`HELP_WEIGHT_LINES`]; see [`crate::game::Game::show_help`] for the
/// order the four groups interleave in.
pub const HELP_PLAIN: [&str; 30] = [
    // 1000:5fb9 cs 0x5870
    "^0 Вначале ты должен выбрать свой характер.",
    // 1000:605d cs 0x5945
    "^0 остальными навыками. Шансы не меняются.",
    // 1000:6076 cs 0x5970
    "^0",
    // 1000:608f cs 0x5973
    "^0 Сила - увеличивает урон, и довавляет 1 здоровья",
    // 1000:60a8 cs 0x59a6
    "^0 Ловкость -  +5% попадания если больше 90% то можно бить дважды.",
    // 1000:60c1 cs 0x59e9
    "^0 Живучесть - 5 здоровья",
    // 1000:60da cs 0x5a03
    "^0 Удача - будет чаще везти по жизни",
    // 1000:60f3 cs 0x5a28
    "^0 ",
    // 1000:610c cs 0x5a2c
    "^0 Здоровье = 10+Живучесть*5+Сила",
    // 1000:6125 cs 0x5a4e
    "^0 Урон = (Сила/2)мин - (Сила)макс урону который нанесёшь ",
    // 1000:613e cs 0x5a89
    "^0 Точность = (20+Ловкость*5)%",
    // 1000:6157 cs 0x5aa8
    "^0 Броня - насколько уменьшается сила вражеского удара.",
    // 1000:6170 cs 0x5970 -- the same literal as index 2, referenced twice.
    "^0",
    // 1000:6189 cs 0x5ae0
    "^0 Спомощью разных вещей можно улутшать навыки, урон, броню и т.д.",
    // 1000:61a2 cs 0x5b23
    "^0 Заходя вразные места, ты узнаешь чего полезного можно из них получить.",
    // 1000:61c2 cs 0x5b6d -- the one gated line, see HELP_DISTRICT_LINE.
    "^0 Придя в новый район, ты должен находить все эти места снова.",
    // 1000:61db cs 0x5bad
    "^0 Хочешь спросить а че от них толку та?",
    // 1000:61f4 cs 0x5bd6
    "^0 Базар    - Покупаешь шмотки и еду. Можно воровать кошельки - Поднимай Удачу",
    // 1000:620d cs 0x5c25
    "^0 Больница - Лечить переломы и царапины",
    // 1000:6226 cs 0x5c4e
    "^0 Подруга  - Ты бомжуешь по кабакам и забыл че такое здоровый образ жизни. У неё",
    // 1000:623f cs 0x5ca0
    "^0            дома ты можешь пожить пару дней как человек - Поправить здоровье",
    // 1000:6258 cs 0x5cef
    "^0 Притон   - Там тусуются реальные пацаны, много че можно сделать, заходи туда",
    // 1000:6271 cs 0x5d3f
    "^0            почаще. А главное ты можешь позвать братву, если дела идут хреново,",
    // 1000:628a cs 0x5d91
    "^0            следи за притонной понтовостью.",
    // 1000:62a3 cs 0x5dbf
    "^0 Клуб     - Можно играть на бабло(Качай Удачу), но тамошние гопники не любят",
    // 1000:62bc cs 0x5e0e
    "^0            проигрывать",
    // 1000:62d5 cs 0x5e28
    "^0 Качалка  - Там можно повысить свои бойцовские навыки",
    // 1000:62ee cs 0x5e60
    "^0 Барыги   - Продают вещи и арсенала гопника, им ты можешь спихнуть награбленый",
    // 1000:6307 cs 0x5eb1
    "^0            хлам и продать ненужные вещи(Выделеные красным в твоей статистике)",
    // 1000:6320 cs 0x5f02
    "^0Если не видать чё вверху - выйди и открой у G.exe:Свойства/Экран/Исходный размер",
];

/// The only branch in the whole of `FUN_1000_5f55`: `1000:61bb
/// cmp byte [0x3692],0x1` / `1000:61c0 jbe 0x61db` skips
/// `HELP_PLAIN[15]` while the district is 1. Every other line is
/// unconditional.
pub const HELP_DISTRICT_LINE: usize = 15;

/// The CS literals of `help`'s two composed lines, in address order:
/// `1000:5f6a` (assign), `5f87`, `5f9b`, `5fd8` (assign), `5ff5`.
///
/// The first line is `[0] + ranks[class] + [1] + name + [2]` -- the rank
/// append at `1000:5f82` reads `DS:(class * 0x100 + 0x2e)`, i.e.
/// [`crate::data::rank_name`], and the name append at `1000:5f96` reads
/// `DS:379c`. The second is `[3] + ranks[class] + [4]`, appended at
/// `1000:5fed`.
pub const HELP_FRAGMENTS: [&str; 5] = [
    "^0Ну слушай, ",
    " ",
    " ^0,в чем тут батва",
    "^0 Например ",
    ":",
];

/// The two `#`-filled `help` lines, `1000:6033` and `1000:6058`. Both push
/// one value and it is the same one.
pub const HELP_WEIGHT_LINES: [&str; 2] = [
    // cs 0x58ab
    "^0 Всего навыки в сумме составляют 12, а сила у тебя # - значит при получении",
    // cs 0x58f9
    "^0 нового уровня понтовости # из 12 шансов что увеличиться сила. То же и с ",
];

/// Which of the class's four growth weights those two lines print.
///
/// `1000:6020 mov al,[di+0x2]` with `di = [0x389c] * 4` -- the weight table
/// is at `DS:0002` and the rows are four bytes, so `+0x2` is the row's
/// FIRST byte: strength. `crate::progress::CLASS_WEIGHTS`'s rows are in the
/// same order, so the displacement's `- 2` is this index.
pub const HELP_WEIGHT_INDEX: usize = 0;

/// `FUN_1000_02c2` -- five blanks, the banner, the version, the prompt, the
/// credit, then `1000:04aa`'s `ReadKey`.
///
/// Called from `1000:6a5a`, between the `GetDir` that builds the save path
/// and the `Randomize` at `1000:6a5d`, so it runs before the save-slot menu
/// on every start. `lines.next()` is this port's line-based `ReadKey`, the
/// same stand-in [`crate::game::Game::enter_district_5`] and
/// [`crate::persist::choose_slot`] use.
pub fn splash(lines: &mut dyn Iterator<Item = io::Result<String>>) {
    play(lines, &SPLASH, SPLASH_GAPS);
}

/// Print `text` with `gaps`' blank lines and `ReadKey`s interleaved.
///
/// The gap table is load-bearing here, not documentation: it is the same
/// object `tools/difftest.py` re-derives from the image, so a wrong blank
/// count or a misplaced `ReadKey` is a failing record rather than a silent
/// difference on screen.
fn play(lines: &mut dyn Iterator<Item = io::Result<String>>, text: &[&str], gaps: Gaps) {
    for i in 0..=text.len() {
        if let Some((_, events)) = gaps.iter().find(|(at, _)| *at == i) {
            for event in events.chars() {
                match event {
                    'B' => term::println(""),
                    'K' => {
                        // `lines.next()` is this port's `ReadKey`: one line
                        // read and discarded, `None` at EOF treated the same
                        // as any other keystroke. The convention is
                        // `Game::enter_district_5`'s and
                        // `persist::choose_slot`'s.
                        let _ = lines.next();
                    }
                    other => unreachable!("gap event {other:?} is not B or K"),
                }
            }
        }
        if let Some(line) = text.get(i) {
            term::println(line);
        }
    }
}

/// `1000:6de6`..`6f2b` -- [`BACKSTORY`] with its seven `ReadKey`s and its
/// one bare `WriteLn`.
///
/// `1000:6de0`'s `mov word [0x38d0],0xa` sits immediately above this block
/// and is already ported as `crate::progress::THRESHOLD_BASE`, so it is not
/// repeated here.
pub fn backstory(lines: &mut dyn Iterator<Item = io::Result<String>>) {
    play(lines, &BACKSTORY, BACKSTORY_GAPS);
}
