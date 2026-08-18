//! Locations and the per-district rediscovery flags.
//!
//! `PLACES.SAV` is 7 bytes, one per rediscoverable location. `orig/PLACES.SAV`
//! is all `0x01` (every location already found), which round-trips correctly
//! under any permutation of the 7 slots -- see the "unverified" note on
//! [`TRACKED`] below.
//!
//! Entering a new district hides every location again; `reset_for_new_district`
//! models that (`docs/re/tables.md`, "Availability gates": `district` is
//! `20ae:3692`, raised once понтовость reaches `district * 10`, file `0xC462`
//! / `1000:ab92`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Street,
    BigMarket,
    Market,
    Vet,
    Girl,
    Den,
    Club,
    Gym,
    Temple,
    Dorm,
}

/// The seven locations tracked by `PLACES.SAV`, in file order.
///
/// **UNVERIFIED.** All five `.SAV` files under `orig/` carry `01` in every
/// slot (`orig/PLACES.SAV` itself is `01 01 01 01 01 01 01`), so the file's
/// own bytes cannot pin down which byte belongs to which location -- the
/// round-trip test below passes under any permutation of this array. Pinning
/// the true order needs the disassembly of the save/load routine that reads
/// `PLACES.SAV`, which this task did not locate (the search for the
/// PLACES.SAV read/load routine's disassembly was not run to completion; see
/// task-11-report.md). The order below is a guess in the same spirit as the
/// brief's -- Market/BigMarket/Vet/Girl/Den/Club/Gym, the order the game's
/// own `mar`/`bmar`/`rep`/`girl`/`pr`/`kl`/`trn` command tokens appear in
/// `data/strings.json` (`docs/re/tables.md`'s cited offsets) -- but that is
/// evidence about the *command table*, not about `PLACES.SAV`'s own byte
/// layout, so it is not being asserted as confirmed.
pub const TRACKED: [Location; 7] = [
    Location::Market,
    Location::BigMarket,
    Location::Vet,
    Location::Girl,
    Location::Den,
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

    pub fn reset_for_new_district(&mut self) {
        self.found = [false; 7];
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

    #[test]
    fn locations_outside_tracked_are_always_found() {
        let places = Places::from_bytes(&[0u8; 7]);
        assert!(places.is_found(Location::Street));
        assert!(places.is_found(Location::Temple));
        assert!(places.is_found(Location::Dorm));
        assert!(!places.is_found(Location::Market));
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
