//! The static tables extracted from `orig/g.exe`, embedded at compile time.
//!
//! Nothing in this module *derives* anything: `data/items.json`,
//! `data/shops.json` and `data/enemies.json` are produced by
//! `tools/extract_tables.py` reading the original binary, and this file only
//! deserialises them. See `docs/re/tables.md` for every Ghidra address and
//! for the DOSBox-X oracle runs that confirmed the prices and the boss stat
//! blocks on the original's own screens.
//!
//! Each loader returns an owned `Vec` rather than a `&'static [T]`, because
//! `serde_json` cannot produce a `'static` slice without a `OnceLock`; add
//! one only if profiling shows the parse matters.

use serde::Deserialize;

use crate::model::Fighter;

/// One piece of equipment the status screen can list.
///
/// `bonus` is the number the original prints in the name itself, e.g.
/// `^1Тесак(Урон+9) `. `effect` is what that suffix literally says the bonus
/// applies to (`damage` / `luck` / `all` / `regen`); the bare `(+N)` form
/// names no stat, so `effect` is `None` there rather than guessed.
///
/// `price` is filled in only where a shop row names the item verbatim with
/// the same bonus; see `link_item_prices` in `tools/extract_tables.py`. It is
/// `None` for everything else, which is honest -- the original's shop text
/// and its inventory text are not the same strings.
#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub bonus: i32,
    pub effect: Option<String>,
    pub price: Option<i32>,
    /// Where the shop price came from, when there is one.
    pub price_src: Option<String>,
    /// File offset of the item's display string in `orig/g.exe`.
    pub src_off: u32,
}

/// One line of a shop menu.
///
/// `price` is the byte the affordability test and the debit both read;
/// `displayed_price` is the byte the row *prints*. They differ for exactly
/// one row in the original -- see `docs/re/tables.md`.
#[derive(Debug, Clone, Deserialize)]
pub struct ShopEntry {
    /// The original's own location tag: `mar` (Базар) or `bmar` (Барыги).
    pub shop: String,
    /// The key the player types to buy this row.
    pub key: String,
    /// The row's source text, `^N` markup and `#` placeholders intact.
    pub text: String,
    pub price: i32,
    pub price_addr: String,
    pub displayed_price: i32,
    pub displayed_price_addr: String,
    /// True when a `sub [money],price` site in the original reads
    /// `price_addr`, i.e. the row is really charged what this field says.
    pub charged: bool,
    /// District requirement, e.g. `district>2`, or `None` when ungated.
    pub gate: Option<String>,
    /// Further conditions guarding the row, as raw memory comparisons.
    pub extra_gates: Vec<String>,
    pub code_addr: String,
}

/// The stat block of a scripted (non-random) enemy.
#[derive(Debug, Clone, Deserialize)]
pub struct EnemyStats {
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub luck: u16,
    pub dmg_min: u16,
    pub dmg_max: u16,
    pub hp: u16,
    pub hpmax: u16,
    pub armor: u16,
}

/// One enemy kind.
///
/// The original has no table of enemy stat blocks: a random encounter picks
/// a class 0..9 and rolls its stats from that class's weight row. So
/// `generated` rows carry `growth_weights` and nothing else, and only the two
/// scripted endgame fights carry `level` and `stats`.
#[derive(Debug, Clone, Deserialize)]
pub struct Enemy {
    pub id: String,
    pub name: String,
    pub class: u16,
    pub level: Option<u16>,
    pub stats: Option<EnemyStats>,
    /// strength / agility / vitality / luck draw weights.
    pub growth_weights: Vec<u16>,
    /// True when the original rolls this enemy rather than scripting it.
    pub generated: bool,
    pub source: String,
}

impl Enemy {
    /// The fighter the original sets up for a scripted encounter, or `None`
    /// for a class whose stats are rolled at runtime.
    pub fn to_fighter(&self) -> Option<Fighter> {
        let s = self.stats.as_ref()?;
        Some(Fighter {
            name: self.name.clone(),
            class: self.class,
            level: self.level.unwrap_or(0),
            hp: s.hp,
            hpmax: s.hpmax,
            strength: s.strength,
            agility: s.agility,
            vitality: s.vitality,
            luck: s.luck,
            armor: s.armor,
            dmg_min: s.dmg_min,
            dmg_max: s.dmg_max,
            ..Fighter::default()
        })
    }
}

static ITEMS_JSON: &str = include_str!("../data/items.json");
static SHOPS_JSON: &str = include_str!("../data/shops.json");
static ENEMIES_JSON: &str = include_str!("../data/enemies.json");

pub fn items() -> Vec<Item> {
    serde_json::from_str(ITEMS_JSON).expect("data/items.json is malformed")
}

pub fn shops() -> Vec<ShopEntry> {
    serde_json::from_str(SHOPS_JSON).expect("data/shops.json is malformed")
}

pub fn enemies() -> Vec<Enemy> {
    serde_json::from_str(ENEMIES_JSON).expect("data/enemies.json is malformed")
}
