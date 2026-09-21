//! Locations and the per-district rediscovery flags.
//!
//! `PLACES.SAV` is 7 bytes, one per rediscoverable location. `orig/PLACES.SAV`
//! is all `0x01` (every location already found), which round-trips correctly
//! under any permutation of the 7 slots, so the file itself cannot pin the
//! order down -- the reader at `1000:6c5a` does, and [`TRACKED`] quotes it.
//!
//! Entering a new district hides locations again; `reset_for_new_district`
//! models that (`docs/re/tables.md`, "Availability gates": `district` is
//! `20ae:3692`, raised once понтовость reaches `district * 10`, file `0xC462`
//! / `1000:ab92`).
//!
//! The reset is NOT unconditional, and `reset_for_new_district` now models
//! that. `1000:ab96` clears Vet and Market, then three `74 05` skips each
//! spare exactly one flag -- Club at `1000:aba7` and Girl at `1000:abb8` are
//! spared when `[20ae:389c] == 3`, Den at `1000:abc9` when it is `5`. Gym at
//! `1000:abac` and Dealers at `1000:abbd` are always cleared, being the
//! second store in each pair, past the skip -- which is the whole reason the
//! guards read as sparing one flag rather than two:
//!
//! ```text
//! 1000:ab96  mov byte [0x3698],0x0    Vet      -- always
//! 1000:ab9b  mov byte [0x3694],0x0    Market   -- always
//! 1000:aba0  cmp word [0x389c],0x3
//! 1000:aba5  jz 0xabac                         -- class 3 skips the next store
//! 1000:aba7  mov byte [0x3699],0x0    Club
//! 1000:abac  mov byte [0x369a],0x0    Gym      -- always (jump lands here)
//! 1000:abb1  cmp word [0x389c],0x3
//! 1000:abb6  jz 0xabbd
//! 1000:abb8  mov byte [0x3697],0x0    Girl
//! 1000:abbd  mov byte [0x3695],0x0    Dealers  -- always
//! 1000:abc2  cmp word [0x389c],0x5
//! 1000:abc7  jz 0xabce
//! 1000:abc9  mov byte [0x3696],0x0    Den
//! ```
//!
//! Re-derive with
//! `python3 tools/re_query.py resolve 1000:ab96 -n 60 -i 30`.
//!
//! `[0x389c]` is the character class (Task 11b), read the same way in
//! `Game::apply_class_bonus` (`1000:73bb`). This was a KNOWN DIVERGENCE for
//! three tasks -- `Places` had no class to consult, and after `Task 21` made
//! `Game::district_advance` the one caller (where `self.player.class` is in
//! scope) the only thing left holding it open was that the brief had scoped
//! it out. The class is now a parameter.

/// `Dealers` is the `bmar` verb -- the game calls the place **Барыги**, not
/// a market. Its entry text is `Ты пришел к барыгам напиши  ^6w^7  чтобы
/// уйти.` (note the double spaces around `^6w^7`) and its prompt is
/// `^0Барыги\`. That is also where the pistol is bought. `mar` is the
/// separate базар, `^6Ты незнаешь, пока ешё, где находтся базар`, so `bmar`
/// is a different location, not a bigger one; the Вор class bonus sets this
/// flag and its menu line calls the bonus `Барыги`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Street,
    Dealers,
    Market,
    Vet,
    Girl,
    Den,
    Club,
    Gym,
    Temple,
    Dorm,
}

/// The seven locations tracked by `PLACES.SAV`, in file order. Loading
/// prints `^0Загружено из places`.
///
/// | verb | location |
/// |---|---|
/// | `mar` | Market |
/// | `bmar` | Dealers |
/// | `pr` | Den |
/// | `girl` | Girl |
/// | `rep` | Vet |
/// | `kl` | Club |
/// | `trn` | Gym |
pub const TRACKED: [Location; 7] = [
    Location::Market,
    Location::Dealers,
    Location::Den,
    Location::Girl,
    Location::Vet,
    Location::Club,
    Location::Gym,
];

#[derive(Debug, Clone)]
pub struct Places {
    found: [bool; 7],
}

impl Places {
    pub fn from_bytes(b: &[u8]) -> Places {
        assert_eq!(b.len(), 7, "PLACES.SAV must be 7 bytes, got {}", b.len());
        let mut found = [false; 7];
        for (i, slot) in found.iter_mut().enumerate() {
            *slot = b[i] != 0;
        }
        Places { found }
    }

    pub fn to_bytes(&self) -> [u8; 7] {
        let mut out = [0u8; 7];
        for (i, &f) in self.found.iter().enumerate() {
            out[i] = u8::from(f);
        }
        out
    }

    /// Hides the rediscoverable locations again on a district promotion,
    /// sparing the three the player's class keeps.
    pub fn reset_for_new_district(&mut self, class: u16) {
        for (i, slot) in self.found.iter_mut().enumerate() {
            let spared = match TRACKED[i] {
                Location::Club | Location::Girl => class == 3,
                Location::Den => class == 5,
                _ => false,
            };
            *slot &= spared;
        }
    }

    /// Whether `loc` has been discovered. A location outside [`TRACKED`]
    /// (`Street`, `Temple`, `Dorm`) is always reported found: `PLACES.SAV`
    /// has no flag for it, and the street is always reachable.
    pub fn is_found(&self, loc: Location) -> bool {
        TRACKED
            .iter()
            .position(|&l| l == loc)
            .map(|i| self.found[i])
            .unwrap_or(true)
    }

    pub fn mark_found(&mut self, loc: Location) {
        if let Some(i) = TRACKED.iter().position(|&l| l == loc) {
            self.found[i] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A class-3 player keeps Club and Girl across a promotion, a class-5
    /// player keeps the Den, and nobody keeps Gym or Dealers.
    #[test]
    fn a_promotion_spares_the_flags_the_class_keeps() {
        let kept = |class| {
            let mut p = Places::from_bytes(&[1u8; 7]);
            p.reset_for_new_district(class);
            let mut out: Vec<Location> =
                TRACKED.iter().copied().filter(|&l| p.is_found(l)).collect();
            out.sort_by_key(|l| format!("{l:?}"));
            out
        };
        assert_eq!(kept(3), vec![Location::Club, Location::Girl]);
        assert_eq!(kept(5), vec![Location::Den]);
        // Every other class clears all seven -- 0, 4 and 6 stand for them.
        for class in [0u16, 1, 2, 4, 6, 7] {
            assert_eq!(kept(class), vec![], "class {class}");
        }
    }

    /// The spare is a SKIP, not a re-mark: a flag the player had not found
    /// stays unfound even when the class would have spared it.
    #[test]
    fn sparing_a_flag_never_sets_one_that_was_clear() {
        let mut p = Places::from_bytes(&[0u8; 7]);
        p.reset_for_new_district(3);
        assert_eq!(p.to_bytes(), [0u8; 7]);
    }

    #[test]
    fn locations_outside_tracked_are_always_found() {
        let places = Places::from_bytes(&[0u8; 7]);
        assert!(places.is_found(Location::Street));
        assert!(places.is_found(Location::Temple));
        assert!(places.is_found(Location::Dorm));
        assert!(!places.is_found(Location::Market));
    }

    /// Den is slot 2 and Vet is slot 4, not the other way round.
    #[test]
    fn tracked_is_the_order_the_places_sav_reader_uses() {
        assert_eq!(
            TRACKED,
            [
                Location::Market,  // 6ca2 -> 0x3694
                Location::Dealers, // 6cb4 -> 0x3695
                Location::Den,     // 6cc6 -> 0x3696
                Location::Girl,    // 6cd8 -> 0x3697
                Location::Vet,     // 6cea -> 0x3698
                Location::Club,    // 6cfc -> 0x3699
                Location::Gym,     // 6d0e -> 0x369a
            ]
        );
    }

    #[test]
    fn mark_found_sets_only_that_slot() {
        let mut places = Places::from_bytes(&[0u8; 7]);
        places.mark_found(Location::Girl);
        assert!(places.is_found(Location::Girl));
        assert!(!places.is_found(Location::Market));
        assert_eq!(places.to_bytes(), [0, 0, 0, 1, 0, 0, 0]);
    }
}
