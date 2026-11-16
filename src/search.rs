//! Alpha-beta search.
//!
//! A [`Worker`] owns all the state one search thread needs, so that several can later
//! search the same position side by side (Lazy SMP). Only one runs for now. The
//! [`TranspositionTable`] is the one thing they share.

mod ordering;
mod score;
mod stop;
mod time;
mod tt;
mod worker;

pub use score::Score;
pub use stop::StopFlag;
pub use time::{Clock, TimeLimit};
pub use tt::TranspositionTable;
pub use worker::{Limits, Report, Worker};

/// Deepest ply the search reaches, counting quiescence search, and the length of the
/// longest principal variation.
const MAX_PLY: usize = 128;
