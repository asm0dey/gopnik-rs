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
//! KNOWN DIVERGENCE, established from flow: the original's reset is NOT
//! unconditional. `1000:ab96` clears Vet and Market, then three `74 05` skips
//! each spare exactly one flag -- Club at `1000:aba7` and Girl at `1000:abb8`
//! are spared when `[20ae:389c] == 3`, Den at `1000:abc9` when it is `5`
//! (Gym at `1000:abac` and Dealers at `1000:abbd` are always cleared, being
//! the second store in each pair, past the skip). `reset_for_new_district`
//! clears all seven unconditionally.
//!
//! `[0x389c]` is no longer the blocker this comment used to name: Task 11b
//! established it as the character class, and Task 11c reads it in
//! `Game::apply_class_bonus` (`1000:73bb`). What is still missing is that
//! `Places` has no class to consult -- the fix is to pass one in, which
//! belongs with the district-transition block (`1000:ab75`..`1000:ae18`)
//! rather than with the wander turn. See `docs/re/gaps.md`.

/// `Dealers` is `20ae:3695`, the `bmar` verb -- the original calls the place
/// **Барыги**, not a market. Named from its own handler's strings: entry text
/// at file `0xAA29`, `Ты пришел к барыгам напиши  ^6w^7  чтобы уйти.` (note
/// the double spaces around `^6w^7` in the binary), and prompt at
/// file `0xAC4B`, `^0Барыги\`, both inside `1000:c4be`'s body, which is also
/// where the pistol is bought (`1000:cd05`). `mar` / `20ae:3694` is the
/// separate базар, file `0xA9F8`,
/// `^6Ты незнаешь, пока ешё, где находтся базар`, so `bmar` is a different
/// location and not a bigger one; the Вор class bonus at `1000:73e0` sets
/// this flag and its menu line calls the bonus `Барыги`
/// (`docs/re/wander.md`). Earlier revisions called it `BigMarket`, read off
/// the verb token alone.
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

/// The seven locations tracked by `PLACES.SAV`, in file order.
///
/// **Established from flow.** `places.sav` is read by the routine at
/// `1000:6c5a`, and it uses `Read`, not `BlockRead` -- seven separate
/// one-byte reads, each naming its destination flag, so the file's byte
/// order is read off the disassembly directly:
///
/// ```text
/// 6c5a  mov di,0x3e36 / push ds / push di   ; the file variable
/// 6c6a  call 0f78:0ae7                      ; build the name from DS:3d32
/// 6c74  call 0f78:0b66                      ; + cs:0x63f2 = file 0x7CC2, `places.sav`
/// 6c79  call 0f78:072e                      ; Assign
/// 6c87  call 0f78:0769                      ; Reset(f, 1)   -- record size 1
/// 6c8c  call 0f78:028a                      ; IOResult; non-zero -> 1000:6d3b
/// 6ca2  call 0f78:081e  -> DS:0x3694        ; Read #1  Market
/// 6cb4  call 0f78:081e  -> DS:0x3695        ; Read #2  Dealers
/// 6cc6  call 0f78:081e  -> DS:0x3696        ; Read #3  Den
/// 6cd8  call 0f78:081e  -> DS:0x3697        ; Read #4  Girl
/// 6cea  call 0f78:081e  -> DS:0x3698        ; Read #5  Vet
/// 6cfc  call 0f78:081e  -> DS:0x3699        ; Read #6  Club
/// 6d0e  call 0f78:081e  -> DS:0x369a        ; Read #7  Gym
/// 6d1b  call 0f78:07ea                      ; Close
/// 6d20  writes `^0Загружено из places` (file 0x7CCD)
/// ```
///
/// So **file order == flag-address order**, and the array below is that
/// order. The seven flags are the contiguous bytes at `20ae:3694..369a`
/// whose gates are disassembled in `docs/re/command-dispatch.md`,
/// "Discovery gates":
///
/// | byte | `20ae:` | verb | location |
/// |---|---|---|---|
/// | 0 | `3694` | `mar` | Market |
/// | 1 | `3695` | `bmar` | Dealers |
/// | 2 | `3696` | `pr` | Den |
/// | 3 | `3697` | `girl` | Girl |
/// | 4 | `3698` | `rep` | Vet |
/// | 5 | `3699` | `kl` | Club |
/// | 6 | `369a` | `trn` | Gym |
///
/// Earlier revisions carried Den and Vet swapped at slots 2 and 4, on the
/// order the `mar`/`bmar`/`rep`/`girl`/`pr`/`kl`/`trn` command tokens appear
/// in `data/strings.json` -- evidence about the *command table*, not about
/// the file. `orig/PLACES.SAV` is `01` in every slot and cannot arbitrate
/// (the round-trip test below passes under any permutation), so the
/// disassembly above is the only thing that settles it, and it does.
///
/// The read's own failure arm (`1000:6d3b`, taken when `IOResult` is
/// non-zero) clears the flags with three `[0x389c]`-keyed exceptions and
/// leaves via `1000:6da0`; it is described in `docs/re/gaps.md`.
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

    /// The order `1000:6ca2`..`1000:6d0e` reads the seven bytes into
    /// `DS:0x3694`..`DS:0x369a`. Den is slot 2 and Vet slot 4, not the
    /// other way round.
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
