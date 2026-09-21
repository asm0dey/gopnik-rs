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
///
/// A `None` price is not always the same kind of unknown: see `sold`.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub bonus: i32,
    pub effect: Option<&'static str>,
    pub price: Option<i32>,
    pub sold: bool,
}

#[derive(Debug, Clone)]
pub struct ShopEntry {
    /// The original's own location tag: `mar` (Базар) or `bmar` (Барыги).
    pub shop: &'static str,
    /// The key the player types to buy this row.
    pub key: &'static str,
    /// The row's source text, `^N` markup and `#` placeholders intact.
    pub text: &'static str,
    pub price: i32,
    pub displayed_price: i32,
    /// District requirement, e.g. `district>2`, or `None` when ungated.
    pub gate: Option<&'static str>,
    /// Further conditions guarding the row, as raw memory comparisons.
    pub extra_gates: &'static [&'static str],
}

/// The stat block of a scripted (non-random) enemy.
#[derive(Debug, Clone)]
pub struct EnemyStats {
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub luck: u16,
    pub dmg_min: u16,
    pub dmg_max: u16,
    pub hp: u16,
    pub hpmax: u16,
    pub armor: u8,
}

/// One enemy kind.
///
/// The original has no table of enemy stat blocks: a random encounter picks
/// a class 0..9 and rolls its stats from that class's weight row. So
/// `generated` rows carry `growth_weights` and nothing else, and only the two
/// scripted endgame fights carry `level` and `stats`.
#[derive(Debug, Clone)]
pub struct Enemy {
    pub id: &'static str,
    pub name: &'static str,
    pub class: u16,
    pub level: Option<u16>,
    pub stats: Option<EnemyStats>,
    /// strength / agility / vitality / luck draw weights.
    pub growth_weights: &'static [u16],
    /// True when the original rolls this enemy rather than scripting it.
    pub generated: bool,
}

impl Enemy {
    /// The fighter the original sets up for a scripted encounter, or `None`
    /// for a class whose stats are rolled at runtime.
    pub fn to_fighter(&self) -> Option<Fighter> {
        let s = self.stats.as_ref()?;
        Some(Fighter {
            name: self.name.to_string(),
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

include!(concat!(env!("OUT_DIR"), "/tables.rs"));

pub fn items() -> &'static [Item] {
    ITEMS
}

pub fn shops() -> &'static [ShopEntry] {
    SHOPS
}

pub fn enemies() -> &'static [Enemy] {
    ENEMIES
}

pub fn rank_name(class: u16) -> &'static str {
    RANKS.get(usize::from(class)).copied().unwrap_or("")
}

/// The крутизна ladder the same header indexes by LEVEL -- DGROUP
/// `20ae:0b42`, 43 entries, stride 256.
///
/// **Established from flow.** `1000:1a53` `mov di,[0x38a6]` (the level),
/// `1000:1a59` `shl di,cl`, `1000:1a5b` `add di,0xb42`, `1000:1a61` appends.
/// Same `""` port decision for an out-of-range index as [`rank_name`]; the
/// level is capped at 40 by `1000:2580` (`crate::progress::MAX_LEVEL`), so
/// 43 rows cover every reachable value.
pub fn krutizna(level: u16) -> &'static str {
    KRUTIZNA.get(usize::from(level)).copied().unwrap_or("")
}
