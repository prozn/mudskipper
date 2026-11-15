//! The signal that tells running searches to stop.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A flag shared between the searches and whoever controls them, raised to make every
/// search stop as soon as it can.
///
/// Clones share the same flag.
#[derive(Clone, Debug, Default)]
pub struct StopFlag(Arc<AtomicBool>);

impl StopFlag {
    /// Returns a new flag, lowered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks every search sharing this flag to stop.
    pub fn raise(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Lowers the flag, so that a new search can run.
    pub fn lower(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    /// Reports whether the flag is raised.
    #[must_use]
    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
