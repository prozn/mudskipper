//! The **Transposition table**: what earlier searches learned about positions, keyed by
//! Zobrist key.
//!
//! Every entry is two `AtomicU64`s, the key and a data word, written without a lock. The
//! key word holds the key XOR the data word, so a probe that reads halves of two
//! different writes sees a key mismatch and treats the entry as absent. That makes the
//! table safe to share between search threads.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use super::score;
use crate::chess::{Key, Move};

/// Entries in a bucket, which fills one 64-byte cache line.
const BUCKET_ENTRIES: usize = 4;

/// Bytes in a megabyte, as the `Hash` option counts them.
const MEGABYTE: usize = 1 << 20;

/// How many plies of depth one generation of age is worth when choosing which entry to
/// replace, so that entries left over from earlier searches give way to new ones.
const AGE_WEIGHT: i32 = 8;

/// How much shallower a new result for a position already stored may be and still replace
/// the old one, unless the new one is exact or the old one is from an earlier search.
const SAME_KEY_DEPTH_MARGIN: u32 = 4;

/// Entries sampled to estimate how full the table is.
const HASHFULL_SAMPLE: usize = 1000;

/// Bits of the generation, which wraps around after this many searches.
const AGE_BITS: u32 = 6;
const AGE_MASK: u8 = (1 << AGE_BITS) - 1;

// Layout of the data word; see `Data`.
const SCORE_SHIFT: u32 = 16;
const EVAL_SHIFT: u32 = 32;
const DEPTH_SHIFT: u32 = 48;
const BOUND_SHIFT: u32 = 56;
const AGE_SHIFT: u32 = 58;
const BOUND_MASK: u64 = 0b11;

/// The stored static evaluation meaning there is none.
const NO_EVAL: i16 = i16::MIN;

/// What the search learned about one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    /// The best move found, if any. It may come from another position with the same key,
    /// however unlikely, so it must be checked before use.
    pub mv: Option<Move>,
    /// The score, relative to the root like every score in the search.
    pub score: i32,
    /// The static evaluation, if it was computed.
    pub eval: Option<i32>,
    /// The depth searched to; quiescence search stores 0.
    pub depth: u32,
    /// How `score` relates to the true score.
    pub bound: Bound,
}

impl Entry {
    /// Reports whether the stored score settles a search of the window `alpha` to
    /// `beta` by failing high or low, so that the search can return it at once. The
    /// caller checks the depth.
    ///
    /// An exact score inside the window does not cut off: it would become part of the
    /// principal variation, whose moves below this position the table does not hold.
    pub(super) fn cuts_off(&self, alpha: i32, beta: i32) -> bool {
        (self.score >= beta && self.bound != Bound::Upper)
            || (self.score <= alpha && self.bound != Bound::Lower)
    }
}

/// How a stored score relates to the position's true score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bound {
    /// The search failed low: the true score is at most the stored one.
    Upper = 1,
    /// The search failed high: the true score is at least the stored one.
    Lower = 2,
    /// The stored score is the true score.
    Exact = 3,
}

impl Bound {
    /// Decodes the two bound bits of a data word; 0 is an empty entry.
    const fn from_bits(bits: u64) -> Option<Self> {
        match bits {
            1 => Some(Self::Upper),
            2 => Some(Self::Lower),
            3 => Some(Self::Exact),
            _ => None,
        }
    }
}

/// One entry: the key XOR the data, and the data. Both are 0 when empty.
#[derive(Debug, Default)]
struct Slot {
    key: AtomicU64,
    data: AtomicU64,
}

impl Slot {
    /// The stored key and data word. The key is wrong if the two words come from
    /// different writes.
    fn read(&self) -> (u64, Data) {
        let data = self.data.load(Ordering::Relaxed);
        (self.key.load(Ordering::Relaxed) ^ data, Data(data))
    }

    fn write(&self, key: u64, Data(data): Data) {
        self.key.store(key ^ data, Ordering::Relaxed);
        self.data.store(data, Ordering::Relaxed);
    }

    fn clear(&self) {
        self.key.store(0, Ordering::Relaxed);
        self.data.store(0, Ordering::Relaxed);
    }
}

/// An entry's data word: from the lowest bit up, the move (16 bits), the score and the
/// static evaluation (16 each), the depth (8), the bound (2, 0 when empty) and the
/// generation it was written in (6). Mate scores count plies from the position.
#[derive(Clone, Copy, Debug)]
struct Data(u64);

impl Data {
    /// Packs `entry`, found `ply` plies from the root, written in `generation`.
    fn new(entry: Entry, ply: usize, generation: u8) -> Self {
        let mv = entry.mv.unwrap_or(Move::NULL).to_bits();
        let score = score::to_table(entry.score, ply) as i16;
        let eval = entry.eval.map_or(NO_EVAL, |eval| {
            eval.clamp(-i32::from(i16::MAX), i32::from(i16::MAX)) as i16
        });
        let depth = entry.depth.min(u32::from(u8::MAX));
        Self(
            u64::from(mv)
                | u64::from(score as u16) << SCORE_SHIFT
                | u64::from(eval as u16) << EVAL_SHIFT
                | u64::from(depth) << DEPTH_SHIFT
                | (entry.bound as u64) << BOUND_SHIFT
                | u64::from(generation) << AGE_SHIFT,
        )
    }

    /// Unpacks the entry for the position found `ply` plies from the root, or `None` if
    /// the slot is empty.
    fn entry(self, ply: usize) -> Option<Entry> {
        let eval = (self.0 >> EVAL_SHIFT) as i16;
        Some(Entry {
            mv: self.mv(),
            score: score::from_table(i32::from((self.0 >> SCORE_SHIFT) as i16), ply),
            eval: (eval != NO_EVAL).then_some(i32::from(eval)),
            depth: self.depth(),
            bound: self.bound()?,
        })
    }

    fn is_empty(self) -> bool {
        self.bound().is_none()
    }

    fn bound(self) -> Option<Bound> {
        Bound::from_bits(self.0 >> BOUND_SHIFT & BOUND_MASK)
    }

    fn mv(self) -> Option<Move> {
        let mv = Move::from_bits(self.0 as u16);
        (!mv.is_null()).then_some(mv)
    }

    fn depth(self) -> u32 {
        u32::from((self.0 >> DEPTH_SHIFT) as u8)
    }

    fn age(self) -> u8 {
        (self.0 >> AGE_SHIFT) as u8 & AGE_MASK
    }
}

/// The entries a key may be stored in, filling one cache line.
#[derive(Debug, Default)]
#[repr(align(64))]
struct Bucket([Slot; BUCKET_ENTRIES]);

const _: () = assert!(size_of::<Bucket>() == 64, "a bucket fills one cache line");

/// The table's storage, shared by every handle.
struct Table {
    buckets: Box<[Bucket]>,
    /// The current search's generation, bumped on every `go`.
    generation: AtomicU8,
}

/// A transposition table, sized in megabytes by the `Hash` option.
///
/// Clones share the same table, so every search thread can use it at once.
#[derive(Clone)]
pub struct TranspositionTable(Arc<Table>);

impl TranspositionTable {
    /// Returns an empty table of `mb` megabytes, or the smallest table if `mb` is 0.
    ///
    /// Any size works, not only powers of two.
    #[must_use]
    pub fn new(mb: usize) -> Self {
        let len = (mb * (MEGABYTE / size_of::<Bucket>())).max(1);
        let buckets = (0..len).map(|_| Bucket::default()).collect();
        Self(Arc::new(Table {
            buckets,
            generation: AtomicU8::new(0),
        }))
    }

    /// Empties the table, so that nothing earlier searches stored is seen again.
    ///
    /// Must not be called while a search is using the table.
    pub fn clear(&self) {
        for slot in self.slots() {
            slot.clear();
        }
        self.0.generation.store(0, Ordering::Relaxed);
    }

    /// Starts a new generation, so that entries from earlier searches are replaced first.
    pub(super) fn new_search(&self) {
        self.0.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns what is stored for the position with `key`, found `ply` plies from the
    /// root, if anything.
    pub(super) fn probe(&self, key: Key, ply: usize) -> Option<Entry> {
        let key = key.bits();
        self.bucket(key).0.iter().find_map(|slot| {
            let (stored_key, data) = slot.read();
            (stored_key == key).then(|| data.entry(ply)).flatten()
        })
    }

    /// Stores `entry` for the position with `key`, found `ply` plies from the root.
    ///
    /// The position's own entry is overwritten unless it is deeper and from this search,
    /// and keeps its move if `entry` has none. Otherwise an empty entry is used, or else
    /// the one with the lowest depth after a penalty for age.
    pub(super) fn store(&self, key: Key, ply: usize, mut entry: Entry) {
        let key = key.bits();
        let generation = self.generation();
        let slots = &self.bucket(key).0;
        let mut victim = &slots[0];
        let mut victim_worth = i32::MAX;
        for slot in slots {
            let (stored_key, data) = slot.read();
            if data.is_empty() {
                victim = slot;
                break;
            }
            if stored_key == key {
                if entry.bound != Bound::Exact
                    && entry.depth + SAME_KEY_DEPTH_MARGIN <= data.depth()
                    && data.age() == generation
                {
                    return;
                }
                entry.mv = entry.mv.or(data.mv());
                victim = slot;
                break;
            }
            let age_distance = generation.wrapping_sub(data.age()) & AGE_MASK;
            let worth = data.depth() as i32 - AGE_WEIGHT * i32::from(age_distance);
            if worth < victim_worth {
                victim = slot;
                victim_worth = worth;
            }
        }
        victim.write(key, Data::new(entry, ply, generation));
    }

    /// How full the table is in permille, counting a sample of entries written during the
    /// current search.
    pub(super) fn hashfull(&self) -> u32 {
        let generation = self.generation();
        let mut sampled = 0;
        let mut used = 0;
        for slot in self.slots().take(HASHFULL_SAMPLE) {
            let (_, data) = slot.read();
            sampled += 1;
            if !data.is_empty() && data.age() == generation {
                used += 1;
            }
        }
        used * 1000 / sampled
    }

    fn generation(&self) -> u8 {
        self.0.generation.load(Ordering::Relaxed) & AGE_MASK
    }

    fn bucket(&self, key: u64) -> &Bucket {
        &self.0.buckets[index(key, self.0.buckets.len())]
    }

    fn slots(&self) -> impl Iterator<Item = &Slot> {
        self.0.buckets.iter().flat_map(|bucket| &bucket.0)
    }
}

impl fmt::Debug for TranspositionTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TranspositionTable")
            .field("buckets", &self.0.buckets.len())
            .field("generation", &self.generation())
            .finish()
    }
}

/// The bucket for `key` in a table of `len` buckets: the high half of their product,
/// which spreads keys evenly over any number of buckets.
fn index(key: u64, len: usize) -> usize {
    ((u128::from(key) * len as u128) >> 64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chess::Board;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn a_stored_entry_is_probed_back_unchanged() {
        let tt = TranspositionTable::new(1);
        let board = Board::from_fen(START).expect("valid FEN");
        let entry = Entry {
            mv: Some(board.legal_moves()[3]),
            score: -137,
            eval: Some(25),
            depth: 9,
            bound: Bound::Lower,
        };
        tt.store(board.key(), 4, entry);
        assert_eq!(tt.probe(board.key(), 4), Some(entry));
        assert_eq!(
            tt.probe(board.key(), 11),
            Some(entry),
            "non-mate scores don't depend on ply"
        );
    }

    fn key(fen: &str) -> Key {
        Board::from_fen(fen).expect("valid FEN").key()
    }

    fn entry(score: i32) -> Entry {
        Entry {
            mv: None,
            score,
            eval: None,
            depth: 3,
            bound: Bound::Exact,
        }
    }

    #[test]
    fn a_different_position_sharing_the_bucket_is_not_returned() {
        // A table this small has one bucket, so every key shares it.
        let tt = TranspositionTable::new(0);
        tt.store(key(START), 0, entry(10));
        let other = key("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        assert_eq!(tt.probe(other, 0), None);
    }

    #[test]
    fn a_torn_entry_is_not_returned() {
        let tt = TranspositionTable::new(0);
        let key = key(START);
        tt.store(key, 0, entry(10));
        // Another thread's write to the same slot got only as far as the data word.
        let slot = &tt.0.buckets[0].0[0];
        slot.data
            .store(Data::new(entry(-55), 0, 0).0, Ordering::Relaxed);
        assert_eq!(tt.probe(key, 0), None);
    }

    #[test]
    fn mate_scores_count_from_the_position_whatever_ply_they_are_probed_at() {
        let tt = TranspositionTable::new(1);
        let key = key(START);
        // Mating 7 plies from the root, found 3 plies from the root: mate in 4 from here.
        tt.store(key, 3, entry(score::mated_in(7).abs()));
        assert_eq!(tt.probe(key, 5).map(|e| e.score), Some(-score::mated_in(9)));
        // Being mated 6 plies from the root, found at ply 2: mated in 4 from here.
        tt.store(key, 2, entry(score::mated_in(6)));
        assert_eq!(tt.probe(key, 0).map(|e| e.score), Some(score::mated_in(4)));
        assert_eq!(tt.probe(key, 2).map(|e| e.score), Some(score::mated_in(6)));
    }

    #[test]
    fn the_index_stays_in_range_for_sizes_that_are_not_powers_of_two() {
        for mb in [3, 100] {
            let tt = TranspositionTable::new(mb);
            let len = tt.0.buckets.len();
            assert_eq!(len, mb * 16_384, "{mb} MB holds 64-byte buckets");
            for key in [0, 1, u64::MAX / 3, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
                assert!(index(key, len) < len, "key {key:#x} in {mb} MB");
            }
            assert_eq!(
                index(u64::MAX, len),
                len - 1,
                "the top key uses the last bucket"
            );
        }
    }

    #[test]
    fn hashfull_counts_entries_from_the_current_search_only() {
        let tt = TranspositionTable::new(0);
        assert_eq!(tt.hashfull(), 0);
        tt.store(key(START), 0, entry(10));
        assert_eq!(tt.hashfull(), 250, "one of the one bucket's four entries");
        tt.new_search();
        assert_eq!(tt.hashfull(), 0);
    }

    #[test]
    fn clear_empties_the_table() {
        let tt = TranspositionTable::new(1);
        let key = key(START);
        tt.store(key, 0, entry(10));
        tt.clear();
        assert_eq!(tt.probe(key, 0), None);
    }

    #[test]
    fn an_exact_score_cuts_off_only_outside_the_window() {
        let exact = entry(50);
        assert!(exact.cuts_off(-100, 50), "fails high");
        assert!(exact.cuts_off(50, 100), "fails low");
        assert!(
            !exact.cuts_off(0, 100),
            "inside the window it would join the principal variation"
        );
        let lower = Entry {
            bound: Bound::Lower,
            ..exact
        };
        assert!(lower.cuts_off(0, 50) && !lower.cuts_off(0, 51));
        let upper = Entry {
            bound: Bound::Upper,
            ..exact
        };
        assert!(upper.cuts_off(50, 100) && !upper.cuts_off(49, 100));
    }
}
