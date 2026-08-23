//! The one definition of a fighter.
//!
//! The field list and its meanings come from the original's own in-memory
//! record, which is byte-identical to the `.SAV` layout Task 5 recovered:
//! the player's record lives at `DS:389c` (`.SAV` offset `0x200`) and the
//! enemy's at `DS:3952`, both in Ghidra's `DATA` block, segment `20ae`.
//! Offsets inside a record, and where each one is read or written:
//!
//! | record | `.SAV` | field | evidence |
//! |---|---|---|---|
//! | `+0x00` | `0x200` | `class` (rank/weight-table index) | `1000:13dc`..`1000:13e4`, `[0x3952] shl 8 + 0x2e` indexes a 256-byte-stride string table; `1000:25aa` indexes the growth-weight table with it; `1000:712a`/`1000:71b8` store it at character creation. See `docs/re/progression.md`. |
//! | `+0x02` | `0x202` | `strength` | `1000:1419` prints it as `Сл:#`; `1000:499a` decrements it with `^4Сила -1` |
//! | `+0x04` | `0x204` | `agility` | same print; drives the blow budget at `1000:3fa7` |
//! | `+0x06` | `0x206` | `vitality` | same print (`Жв:#`) |
//! | `+0x08` | `0x208` | `luck` | same print (`Уд:#`); drives crit and break rolls |
//! | `+0x0a` | `0x20a` | `level` | pushed as the `#` of `# уровня` at `1000:1404`; incremented at `1000:258a` on level-up, capped at 40 by `1000:2580` |
//! | `+0x0c` | `0x20c` | `dmg_min` | printed as `Урон #-#` at `1000:1436` |
//! | `+0x0e` | `0x20e` | `dmg_max` | same print |
//! | `+0x10` | `0x210` | `hp` | printed as `Здоровье #/#` at `1000:1542` |
//! | `+0x12` | `0x212` | `hpmax` | same print |
//! | `+0x14` | `0x214` | `broken_jaw` (byte) | set at `1000:45be` / `1000:47ee` |
//! | `+0x15` | `0x215` | `broken_leg` (byte) | set at `1000:45e5` / `1000:4842` |
//! | `+0x16` | `0x216` | `armor` (byte) | subtracted from damage at `1000:4769`; printed as `^2Броня #` at `1000:163f` |
//!
//! The rolled-enemy record at `DS:3952` stops there and then carries three
//! **loot** words the player's record keeps elsewhere. `1000:523e`..`1000:5251`,
//! the victory block of `FUN_1000_3d11`, is what names them: `[0x396a]` is
//! added into `[0x38c3]` (beer in half-litres), `[0x396c]` into `[0x38c7]`
//! (money) and `[0x396e]` into `[0x38c9]` (Хлам). All three are rolled by
//! `FUN_1000_0d14` -- see [`crate::game::Game::roll_enemy`].
//!
//! The in-game help screen (`1000:610c`..`1000:613e`) states the derived
//! quantities the same way: `Здоровье = 10+Живучесть*5+Сила`,
//! `Урон = (Сила/2)мин - (Сила)макс`, `Точность = (20+Ловкость*5)%`. Both
//! `dmg_min`/`dmg_max` and `armor` are stored, not recomputed, because
//! equipment adds to them.
//!
//! `level` is the game's *понтовость*, and it is **not** the word at `.SAV`
//! offset `0x200`: that word is the rank-name index. Cross-checked three
//! ways on `SAVE_R2` -- the record holds `10` at `+0x0a`, the game prints
//! `10 уровня`, and its next-level threshold is 110, which is
//! `10 + 10 * level` for `level = 10` (`1000:2550`).

/// One combatant. Only the fields above `joints` take part in combat.
///
/// `Default` is every field zeroed and the name empty -- deliberately not a
/// playable fighter, so a caller who forgets to fill one in gets an obvious
/// 0-HP nobody rather than a plausible-looking one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fighter {
    pub name: String,
    /// `+0x00` / `.SAV 0x200` -- indexes both the rank-name table and the
    /// growth-weight table `crate::progress::CLASS_WEIGHTS`. Moved here from
    /// `crate::progress::Progress` (Task 9b fix wave 1): it is part of the
    /// same 16-byte record every other field below mirrors, not XP
    /// bookkeeping kept outside it.
    pub class: u16,
    pub level: u16,
    pub hp: u16,
    pub hpmax: u16,
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub luck: u16,
    pub armor: u16,
    pub dmg_min: u16,
    pub dmg_max: u16,
    pub broken_jaw: bool,
    pub broken_leg: bool,
    // Inventory/status fields. Not used by combat, but declared here so
    // the struct is defined exactly once; Task 11's handlers rely on them.
    pub joints: u16,
    pub stoned: bool,
    pub beer_dl: u16,
    pub money: i32,
    /// `DS:38c9` for the player, `DS:396e` for a rolled enemy -- "Хлам",
    /// the junk the dealers buy back (`1000:ce87`..`1000:ce97` moves it into
    /// the money at `DS:38c7` and zeroes it; the stat block prints it as
    /// `Хлам #` at `1000:246a`). A rolled opponent carries some, and
    /// `1000:524c` (`mov ax,[0x396e]` / `add [0x38c9],ax`) hands it to the
    /// winner.
    pub junk: u16,
}
