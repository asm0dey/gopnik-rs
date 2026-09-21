//! The game's opening and its two other straight-line text dumps.
//!
//! | block | what |
//! |---|---|
//! | [`SPLASH`] | the title screen |
//! | [`BACKSTORY`] | the university cold open, before the class menu |
//! | [`START_ARRIVAL`] / [`TUTORIAL`] | the district line at start-up, and district 1's three-line crib |
//! | [`ADVANCE_ARRIVAL`] | the same district line when play promotes you |
//! | [`QUIT_TAIL`] | what `e` / `exit` prints on the way out |
//! | [`HELP_PLAIN`] / [`HELP_FRAGMENTS`] | the whole `help` verb |
//!
//! ## The district lines exist twice
//!
//! [`START_ARRIVAL`]'s districts 2/3/4 and all of [`ADVANCE_ARRIVAL`] read
//! the same on screen but are kept as **separate string copies**, the same
//! decision [`crate::game::Game::announce_district`] and
//! [`crate::game::Game::enter_district_5`] already make for the
//! district-5 line.
//!
//! ## Screen control is dropped, as everywhere else in this port
//!
//! Colour changes and screen clears around the splash are dropped, since
//! `crate::term` has no persistent attribute and no screen -- the same way
//! `crate::ending`'s colour changes and clears were dropped. The `ReadKey`
//! between them is kept.

use std::io;

use crate::term;

/// What one of these blocks does in the gaps *between* its literal lines.
///
/// Each entry is `(index, events)`: the events emitted after line
/// `index - 1` and before line `index`, with `index == len()` meaning
/// "after the last line, before the block ends". `'B'` is a bare blank
/// line and `'K'` is a wait for a keypress. Gaps with no events are
/// **omitted**, so an empty table means a block that runs straight
/// through.
///
/// `'C'` is a third shape [`crate::church`] needs and this module's blocks
/// never use: a line assembled from pieces at the call site rather than
/// one fixed literal, so the caller supplies the built string.
pub type Gaps = &'static [(usize, &'static str)];

/// The blocks that run straight through: no blank line and no `ReadKey`
/// anywhere inside them.
pub const NO_GAPS: Gaps = &[];

/// The eleven literals: eight banner rows (one colour digit each), the
/// version line, the prompt, and the credit.
///
/// The blank `WriteLn`s between them are [`splash`]'s business, not the
/// array's: they carry no literal.
pub const SPLASH: [&str; 11] = [
    "                  ^0┌──── ┌────┐ ┌────┐  │    │  │    │  │    /",
    "                  ^1│     │    │ │    │  │    │  │    │  │   / ",
    "                  ^2│     │    │ │    │  │    │  │    │  │  /  ",
    "                  ^3│     │    │ │    │  ├────┤  │   /│  │_/   ",
    "                  ^4│     │    │ │    │  │    │  │  / │  │ \\   ",
    "                  ^5│     │    │ │    │  │    │  │ /  │  │  \\  ",
    "                  ^6│     │    │ │    │  │    │  │/   │  │   \\ ",
    "                  ^7│     └────┘ │    │  │    │  │    │  │    \\",
    "                                                ^0Версия 1.02",
    "                          ^6Нажми какую-нибудь кнопку",
    "     ^02003 year, June,Sept                                         ^2by V.P.",
];

/// What the splash does in the gaps between its literals -- five blank
/// lines before the first banner row, one before the version line, three
/// before the prompt, four before the credit, and a `ReadKey` after it.
/// See [`Gaps`].
pub const SPLASH_GAPS: Gaps = &[(0, "BBBBB"), (8, "B"), (9, "BBB"), (10, "BBBB"), (11, "K")];

/// The new-character path's cold open: eleven lines, printed before the
/// class menu runs next.
pub const BACKSTORY: [&str; 11] = [
    "Год 2xxx от Р.Х.",
    "Последний день ты пришел в универ",
    "Ты по-страшному косил и забивал",
    "Ты ещё мог сдать все задания, которые ты взял у друзей",
    "Но тут...",
    "^6Ректор: Ах ты урод, чёртов забивала. Вали из универа!",
    "^2Ты: А типа чё?",
    "^6Ректор: Ты отчислен мудак!!! Как ты был лохом так и останешься.",
    "^4Это слышали все и ты из пацана превратился в опушенного.",
    "Ты неможешь стерпеть такой наезд, однако ректор офигительно крутой.",
    "Ты решил доказать свою крутизну всему миру(в твоем понимании - Городу).",
];

/// The backstory's seven `ReadKey`s and its one bare `WriteLn`, which falls
/// in the same gap as the sixth `ReadKey` and AFTER it -- hence `"KB"`, not
/// `"BK"`. See [`Gaps`].
pub const BACKSTORY_GAPS: Gaps = &[
    (1, "K"),
    (5, "K"),
    (6, "K"),
    (7, "K"),
    (8, "K"),
    (9, "KB"),
    (11, "K"),
];

/// The district line printed once, on the way into the game, for
/// districts 1 to 4. Two lines per district, so district `d` is
/// `[(d - 1) * 2]` and the entry after it.
///
/// District 5's own line is not here: it is written out separately in
/// [`crate::game::Game::announce_district`], a separate copy of the
/// string [`crate::game::Game::enter_district_5`] prints.
pub const START_ARRIVAL: [&str; 8] = [
    "^1Ты стоишь у дверей университета.",
    "^1Отсюда ты начнешь свой нелёгкий путь гопника.",
    "^1Ты сел на автобус и попёрся на шлюз...",
    "^1Там бродит шлюзовская шпана.",
    "^1На маршрутке ты доехал до ОбьГЭСа...",
    "^1Здесь бродит уже более крутая гопота.",
    "^1Ты приехал в Ельцовку...",
    "^1Ото всюду доносятся крики запинываемых.",
];

/// Three more lines, shown only on a district-1 start.
pub const TUTORIAL: [&str; 3] = [
    "Доказать свою крутизну ты можешь, отпинывая разных мудаков.",
    "Тебе придётся поработать над сабой, чтобы стать крутым.",
    "Введи ^6i^7 чтобы посмотреть команды, введи ^6help^7 что бы узнать чё за батва.",
];

/// The district line again, this time on the turn play promotes you.
/// Districts 2, 3 and 4 only, so district `d` is `[(d - 2) * 2]` and the
/// entry after it; district 1 is unreachable here (the promotion
/// increments first) and district 5 is
/// [`crate::game::Game::enter_district_5`].
///
/// A **separate string copy** from [`START_ARRIVAL`]'s districts 2/3/4 --
/// see the module doc.
pub const ADVANCE_ARRIVAL: [&str; 6] = [
    "^1Ты сел на автобус и попёрся на шлюз...",
    "^1Там бродит шлюзовская шпана.",
    "^1На маршрутке ты доехал до ОбьГЭСа...",
    "^1Здесь бродит уже более крутая гопота.",
    "^1Ты приехал в Ельцовку...",
    "^1Ото всюду доносятся крики запинываемых.",
];

/// What `e` / `exit` at the STREET prompt prints before the process ends:
/// two lines, then the full character sheet, then a `ReadKey`.
///
/// The `ReadKey` is [`QUIT_GAPS`]; the character-sheet call sits between
/// the second line and it.
pub const QUIT_TAIL: [&str; 2] = ["^6Блин не быть тебе нормальным пацаном", "^1А результат:"];

/// The one `ReadKey`, after the character sheet.
pub const QUIT_GAPS: Gaps = &[(2, "K")];

/// `help`'s thirty un-composed lines, in address order. The two composed
/// lines and the two `#`-filled ones are [`HELP_FRAGMENTS`] and
/// [`HELP_WEIGHT_LINES`]; see [`crate::game::Game::show_help`] for the
/// order the four groups interleave in.
pub const HELP_PLAIN: [&str; 30] = [
    "^0 Вначале ты должен выбрать свой характер.",
    "^0 остальными навыками. Шансы не меняются.",
    "^0",
    "^0 Сила - увеличивает урон, и довавляет 1 здоровья",
    "^0 Ловкость -  +5% попадания если больше 90% то можно бить дважды.",
    "^0 Живучесть - 5 здоровья",
    "^0 Удача - будет чаще везти по жизни",
    "^0 ",
    "^0 Здоровье = 10+Живучесть*5+Сила",
    "^0 Урон = (Сила/2)мин - (Сила)макс урону который нанесёшь ",
    "^0 Точность = (20+Ловкость*5)%",
    "^0 Броня - насколько уменьшается сила вражеского удара.",
    // The same literal as index 2, referenced twice.
    "^0",
    "^0 Спомощью разных вещей можно улутшать навыки, урон, броню и т.д.",
    "^0 Заходя вразные места, ты узнаешь чего полезного можно из них получить.",
    // The one gated line, see HELP_DISTRICT_LINE.
    "^0 Придя в новый район, ты должен находить все эти места снова.",
    "^0 Хочешь спросить а че от них толку та?",
    "^0 Базар    - Покупаешь шмотки и еду. Можно воровать кошельки - Поднимай Удачу",
    "^0 Больница - Лечить переломы и царапины",
    "^0 Подруга  - Ты бомжуешь по кабакам и забыл че такое здоровый образ жизни. У неё",
    "^0            дома ты можешь пожить пару дней как человек - Поправить здоровье",
    "^0 Притон   - Там тусуются реальные пацаны, много че можно сделать, заходи туда",
    "^0            почаще. А главное ты можешь позвать братву, если дела идут хреново,",
    "^0            следи за притонной понтовостью.",
    "^0 Клуб     - Можно играть на бабло(Качай Удачу), но тамошние гопники не любят",
    "^0            проигрывать",
    "^0 Качалка  - Там можно повысить свои бойцовские навыки",
    "^0 Барыги   - Продают вещи и арсенала гопника, им ты можешь спихнуть награбленый",
    "^0            хлам и продать ненужные вещи(Выделеные красным в твоей статистике)",
    "^0Если не видать чё вверху - выйди и открой у G.exe:Свойства/Экран/Исходный размер",
];

/// `HELP_PLAIN[15]` is skipped while the district is 1. Every other line
/// is unconditional.
pub const HELP_DISTRICT_LINE: usize = 15;

/// `help`'s two composed lines. The first is
/// `[0] + rank + [1] + name + [2]`, with the rank from
/// [`crate::data::rank_name`] and the name the player's own. The second is
/// `[3] + rank + [4]`.
pub const HELP_FRAGMENTS: [&str; 5] = [
    "^0Ну слушай, ",
    " ",
    " ^0,в чем тут батва",
    "^0 Например ",
    ":",
];

/// The two `#`-filled `help` lines both print the same value.
pub const HELP_WEIGHT_LINES: [&str; 2] = [
    // cs 0x58ab
    "^0 Всего навыки в сумме составляют 12, а сила у тебя # - значит при получении",
    // cs 0x58f9
    "^0 нового уровня понтовости # из 12 шансов что увеличиться сила. То же и с ",
];

/// Those two lines print the class's strength growth weight -- the first
/// entry of [`crate::progress::CLASS_WEIGHTS`]'s row for this class.
pub const HELP_WEIGHT_INDEX: usize = 0;

/// Five blanks, the banner, the version, the prompt, the credit, then a
/// `ReadKey`. Runs before the save-slot menu on every start.
///
/// `lines.next()` is this port's line-based `ReadKey`, the same stand-in
/// [`crate::game::Game::enter_district_5`] and
/// [`crate::persist::choose_slot`] use.
pub fn splash(lines: &mut dyn Iterator<Item = io::Result<String>>) {
    play(lines, &SPLASH, SPLASH_GAPS, None);
}

/// Print `text` with `gaps`' blank lines, `ReadKey`s and composed lines
/// interleaved.
///
/// `composed` is the already-built text for the `'C'` event -- the one
/// line in a block that is assembled rather than quoted. A table with a
/// `'C'` and no `composed` is a caller bug and panics rather than dropping
/// the line.
pub fn play(
    lines: &mut dyn Iterator<Item = io::Result<String>>,
    text: &[&str],
    gaps: Gaps,
    composed: Option<&str>,
) {
    for i in 0..=text.len() {
        if let Some((_, events)) = gaps.iter().find(|(at, _)| *at == i) {
            for event in events.chars() {
                match event {
                    'B' => term::println(""),
                    'C' => {
                        term::println(composed.expect("a 'C' gap event needs the composed line"))
                    }
                    'K' => {
                        // `lines.next()` is this port's `ReadKey`: one line
                        // read and discarded, `None` at EOF treated the same
                        // as any other keystroke. The convention is
                        // `Game::enter_district_5`'s and
                        // `persist::choose_slot`'s.
                        term::read_key(lines);
                    }
                    other => unreachable!("gap event {other:?} is not B, K or C"),
                }
            }
        }
        if let Some(line) = text.get(i) {
            term::println(line);
        }
    }
}

/// [`BACKSTORY`] with its seven `ReadKey`s and one bare `WriteLn`.
///
/// The threshold base ([`crate::progress::THRESHOLD_BASE`]) is set
/// immediately before this and is not repeated here.
pub fn backstory(lines: &mut dyn Iterator<Item = io::Result<String>>) {
    play(lines, &BACKSTORY, BACKSTORY_GAPS, None);
}
