//! The UCI options the engine offers, and `setoption`.

use std::fmt;
use std::io::{self, Write};
use std::time::Duration;

use super::Mode;

/// A UCI option of type `spin`: an integer within a range.
struct Spin {
    name: &'static str,
    default: u32,
    min: u32,
    max: u32,
}

impl Spin {
    /// Parses `value` as a setting of this option.
    fn parse(&self, value: Option<&str>) -> Result<u32, SetOptionError> {
        let expected = || format!("an integer from {} to {}", self.min, self.max);
        let value = value.ok_or_else(|| SetOptionError::MissingValue(expected()))?;
        value
            .parse()
            .ok()
            .filter(|n| (self.min..=self.max).contains(n))
            .ok_or_else(|| SetOptionError::InvalidValue {
                value: value.to_owned(),
                expected: expected(),
            })
    }
}

/// Size of the transposition table in megabytes. There is no transposition table yet.
const HASH: Spin = Spin {
    name: "Hash",
    default: 16,
    min: 1,
    max: 65_536,
};

/// Number of search threads. Only one is used for now.
const THREADS: Spin = Spin {
    name: "Threads",
    default: 1,
    min: 1,
    max: 1024,
};

/// Milliseconds kept back from every move's time budget, to cover communication delays
/// between the engine and the GUI or match runner.
const MOVE_OVERHEAD: Spin = Spin {
    name: "Move Overhead",
    default: 10,
    min: 0,
    max: 5000,
};

/// Name of the check option that switches Chess960 mode on and off.
const CHESS960: &str = "UCI_Chess960";

/// Name of the button that clears the transposition table.
const CLEAR_HASH: &str = "Clear Hash";

/// The current setting of every option.
#[derive(Debug)]
pub(super) struct Options {
    /// Whether `UCI_Chess960` is on.
    pub mode: Mode,
    /// The time kept back from every move's time budget.
    pub move_overhead: Duration,
    /// The transposition table size in megabytes, kept for when there is one.
    pub hash_mb: u32,
    /// The number of search threads asked for, kept for when more than one can be used.
    pub threads: u32,
}

impl Options {
    /// Returns every option at its default.
    pub fn new() -> Self {
        Self {
            mode: Mode::Standard,
            move_overhead: Duration::from_millis(MOVE_OVERHEAD.default.into()),
            hash_mb: HASH.default,
            threads: THREADS.default,
        }
    }

    /// Writes the `option` lines of the reply to `uci`.
    pub fn write_uci(output: &mut impl Write) -> io::Result<()> {
        writeln!(output, "option name {CHESS960} type check default false")?;
        for spin in [&HASH, &THREADS, &MOVE_OVERHEAD] {
            writeln!(
                output,
                "option name {} type spin default {} min {} max {}",
                spin.name, spin.default, spin.min, spin.max
            )?;
        }
        writeln!(output, "option name {CLEAR_HASH} type button")
    }

    /// Applies a `setoption` command, and says whether the caller has more to do.
    ///
    /// Option names are matched ignoring case, as the UCI protocol asks. `Clear Hash`
    /// changes no option: it returns [`Applied::ClearHash`] so that the caller, which owns
    /// the search state, resets it. On error no option changes.
    pub fn set(&mut self, line: &str) -> Result<Applied, SetOptionError> {
        let args: Vec<&str> = line.split_whitespace().skip(1).collect();
        let Some((&"name", rest)) = args.split_first() else {
            return Err(SetOptionError::MissingName);
        };
        let (name, value) = match rest.iter().position(|&arg| arg == "value") {
            Some(i) => (rest[..i].join(" "), Some(rest[i + 1..].join(" "))),
            None => (rest.join(" "), None),
        };
        let value = value.as_deref();
        let is = |option: &str| name.eq_ignore_ascii_case(option);
        if is(CHESS960) {
            self.mode = match value {
                Some("true") => Mode::Chess960,
                Some("false") => Mode::Standard,
                Some(value) => {
                    return Err(SetOptionError::InvalidValue {
                        value: value.to_owned(),
                        expected: "true or false".to_owned(),
                    });
                }
                None => return Err(SetOptionError::MissingValue("true or false".to_owned())),
            };
        } else if is(HASH.name) {
            self.hash_mb = HASH.parse(value)?;
        } else if is(THREADS.name) {
            self.threads = THREADS.parse(value)?;
        } else if is(MOVE_OVERHEAD.name) {
            self.move_overhead = Duration::from_millis(MOVE_OVERHEAD.parse(value)?.into());
        } else if is(CLEAR_HASH) {
            if let Some(value) = value {
                return Err(SetOptionError::UnexpectedValue(value.to_owned()));
            }
            return Ok(Applied::ClearHash);
        } else {
            return Err(SetOptionError::UnknownOption(name));
        }
        Ok(Applied::Stored)
    }
}

/// What a successful `setoption` did.
#[must_use = "Clear Hash does nothing unless the caller resets the search state"]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Applied {
    /// An option's new value was stored.
    Stored,
    /// The `Clear Hash` button was pressed: the caller must reset the search state.
    ClearHash,
}

/// Why a `setoption` command was rejected.
#[derive(Debug)]
pub(super) enum SetOptionError {
    /// No `name` followed `setoption`.
    MissingName,
    /// The engine has no option of this name.
    UnknownOption(String),
    /// The option was given no value; it expects the one described.
    MissingValue(String),
    /// The value is not valid for the option.
    InvalidValue {
        /// The value given.
        value: String,
        /// A description of the values the option accepts.
        expected: String,
    },
    /// A button was given a value.
    UnexpectedValue(String),
}

impl fmt::Display for SetOptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingName => f.write_str("expected setoption name <name> [value <value>]"),
            Self::UnknownOption(name) => write!(f, "unknown option {name:?}"),
            Self::MissingValue(expected) => write!(f, "expected value {expected}"),
            Self::InvalidValue { value, expected } => {
                write!(f, "expected {expected}, got {value:?}")
            }
            Self::UnexpectedValue(value) => write!(f, "a button takes no value, got {value:?}"),
        }
    }
}
