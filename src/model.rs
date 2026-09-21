//! The one definition of a fighter.
//!
//! Fields and how they print:
//!
//! - `class` -- indexes the rank name and the growth-weight table.
//! - `strength` -- `Сл:#`; a decrement prints `^4Сила -1`.
//! - `agility` -- same print; drives the blow budget.
//! - `vitality` -- same print (`Жв:#`).
//! - `luck` -- same print (`Уд:#`); drives crit and break rolls.
//! - `level` -- the `#` of `# уровня`; capped at 40.
//! - `dmg_min` / `dmg_max` -- `Урон #-#`.
//! - `hp` / `hpmax` -- `Здоровье #/#`.
//! - `broken_jaw` / `broken_leg` -- byte flags.
//! - `armor` -- subtracted from damage; printed as `^2Броня #`.
//!
//! A rolled enemy also carries three **loot** fields the player's record
//! keeps elsewhere: beer (half-litres), money, and Хлам (junk), all rolled
//! by [`crate::game::Game::roll_enemy`].
//!
//! The in-game help screen states the derived quantities:
//! `Здоровье = 10+Живучесть*5+Сила`, `Урон = (Сила/2)мин - (Сила)макс`,
//! `Точность = (20+Ловкость*5)%`. Both `dmg_min`/`dmg_max` and `armor` are
//! stored, not recomputed, because equipment adds to them.
//!
//! `level` is the game's *понтовость*, and is **not** the same as `class`
//! (the rank-name index). The next-level threshold is `10 + 10 * level`.

/// One combatant. Only the fields above `joints` take part in combat.
///
/// `Default` is every field zeroed and the name empty -- deliberately not a
/// playable fighter, so a caller who forgets to fill one in gets an obvious
/// 0-HP nobody rather than a plausible-looking one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fighter {
    pub name: String,
    /// Indexes both the rank-name table and the growth-weight table
    /// `crate::progress::CLASS_WEIGHTS`. Part of the same record every
    /// other field below mirrors, not XP bookkeeping kept outside it.
    pub class: u16,
    pub level: u16,
    pub hp: u16,
    pub hpmax: u16,
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub luck: u16,
    pub armor: u8,
    pub dmg_min: u16,
    pub dmg_max: u16,
    pub broken_jaw: bool,
    pub broken_leg: bool,
    // Inventory/status fields. Not used by combat, but declared here so
    // the struct is defined exactly once; other handlers rely on them.
    pub joints: i16,
    pub stoned: bool,
    pub beer_dl: i16,
    pub money: i16,
    /// "Хлам" -- the junk the dealers buy back; selling it moves it into
    /// the money and zeroes it. Printed as `Хлам #`. A rolled opponent
    /// carries some, and it is handed to the winner on victory.
    pub junk: i16,
}
