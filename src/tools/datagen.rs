//! **Training data** by self-play, written in viriformat for bullet to train on.
//!
//! Built only with the `datagen` feature.

mod game;
mod record;
mod rng;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use game::{Settings, play_game};
pub use record::{GameRecord, Outcome};
pub use rng::Rng;

use crate::search::{StopFlag, TranspositionTable};

/// Hash size in MB of each thread's transposition table, cleared before every game.
const HASH_MB: usize = 8;

/// How often progress is printed.
const REPORT_INTERVAL: Duration = Duration::from_secs(10);

/// How often the progress loop wakes to see whether the threads have finished.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How many games to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameLimit {
    /// Play this many games in total, across every thread.
    Games(u64),
    /// Play until the stop flag is raised.
    UntilStopped,
}

/// Plays self-play games on `threads` threads until `limit` is reached or `stop` is
/// raised, writing thread `i`'s games to `<out_dir>/<i>.vf`, and prints progress to
/// `output`.
///
/// Each thread plays one game at a time. The output files are appended to, so earlier
/// data in `out_dir` is kept. A raised `stop` lets each thread finish its current game.
/// Progress lines give games, positions and positions per second, every ten seconds and
/// once at the end.
///
/// # Errors
///
/// Returns any I/O error from creating `out_dir`, writing a game or writing `output`.
///
/// # Panics
///
/// Panics if a thread panics.
pub fn run(
    threads: usize,
    limit: GameLimit,
    out_dir: &Path,
    stop: &StopFlag,
    output: &mut impl Write,
) -> io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let files: Vec<File> = (0..threads)
        .map(|index| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(out_dir.join(format!("{index}.vf")))
        })
        .collect::<io::Result<_>>()?;
    let progress = Progress::default();
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    writeln!(output, "datagen threads {threads} seed {seed}")?;

    thread::scope(|scope| {
        let handles: Vec<_> = files
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let progress = &progress;
                let rng = Rng::for_thread(seed, index);
                scope.spawn(move || play_games(file, rng, limit, stop, progress))
            })
            .collect();

        let start = Instant::now();
        let mut last_report = start;
        while !handles.iter().all(thread::ScopedJoinHandle::is_finished) {
            thread::sleep(POLL_INTERVAL);
            if last_report.elapsed() >= REPORT_INTERVAL {
                last_report = Instant::now();
                progress.report(start.elapsed(), output)?;
            }
        }
        for handle in handles {
            handle.join().expect("datagen thread panicked")?;
        }
        progress.report(start.elapsed(), output)
    })
}

/// One thread's work: plays games and appends each to `file` until `limit` is reached or
/// `stop` is raised.
fn play_games(
    mut file: File,
    mut rng: Rng,
    limit: GameLimit,
    stop: &StopFlag,
    progress: &Progress,
) -> io::Result<()> {
    let settings = Settings::default();
    let tt = TranspositionTable::new(HASH_MB);
    let mut bytes = Vec::new();
    while !stop.is_raised() && progress.claim_game(limit) {
        let (game, outcome) = loop {
            if let Some(played) = play_game(&mut rng, &settings, &tt) {
                break played;
            }
        };
        // One write per game, so that an interrupted run leaves whole games behind.
        bytes.clear();
        game.write(outcome, &mut bytes)?;
        file.write_all(&bytes)?;
        progress.record(game.len());
    }
    Ok(())
}

/// Counters shared by every thread.
#[derive(Debug, Default)]
struct Progress {
    /// Games started, counted against the [`GameLimit`].
    started: AtomicU64,
    /// Games written.
    games: AtomicU64,
    /// Positions written, one per recorded move.
    positions: AtomicU64,
}

impl Progress {
    /// Claims the next game, returning false once `limit` games have been claimed.
    fn claim_game(&self, limit: GameLimit) -> bool {
        match limit {
            GameLimit::UntilStopped => true,
            GameLimit::Games(games) => self.started.fetch_add(1, Ordering::Relaxed) < games,
        }
    }

    /// Counts a written game of `positions` positions.
    fn record(&self, positions: usize) {
        self.games.fetch_add(1, Ordering::Relaxed);
        self.positions
            .fetch_add(positions as u64, Ordering::Relaxed);
    }

    /// Prints the games and positions written so far, and positions per second over
    /// `elapsed`.
    fn report(&self, elapsed: Duration, output: &mut impl Write) -> io::Result<()> {
        let games = self.games.load(Ordering::Relaxed);
        let positions = self.positions.load(Ordering::Relaxed);
        let per_second = positions as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
        writeln!(
            output,
            "games {games} positions {positions} positions/s {per_second:.0} elapsed {}s",
            elapsed.as_secs()
        )?;
        output.flush()
    }
}
