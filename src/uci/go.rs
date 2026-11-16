//! Parsing the `go` command.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use crate::chess::Colour;
use crate::search::{Clock, Limits, TimeLimit};

/// What a `go` command asks for.
#[derive(Debug)]
pub(super) enum Go {
    /// `go perft <depth>`: count the leaf nodes below each legal move.
    Perft(u32),
    /// Search for the best move.
    Search {
        /// What the search should do.
        request: SearchRequest,
        /// The parameters that were dropped because they were unsupported or invalid, in
        /// the order given.
        ignored: Vec<GoError>,
    },
}

/// A `go` command that asks for a search.
#[derive(Debug, Default)]
pub(super) struct SearchRequest {
    /// The depth and node limits.
    pub limits: Limits,
    /// The time to search for, from `movetime`.
    pub movetime: Option<Duration>,
    /// Each side's clock, from `wtime`, `btime`, `winc` and `binc`, indexed by colour.
    pub clocks: [SideClock; 2],
    /// The moves left until the next time control, from `movestogo`.
    pub moves_to_go: Option<u32>,
    /// Whether `infinite` was given: the `bestmove` must wait for `stop` even if the
    /// search ends sooner.
    pub infinite: bool,
}

/// One side's clock as a `go` command gives it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SideClock {
    /// The time left, from `wtime` or `btime`.
    pub time: Option<Duration>,
    /// The time gained after each move, from `winc` or `binc`.
    pub increment: Option<Duration>,
}

impl SearchRequest {
    /// The clock of `colour`, for filling in as the command is parsed.
    fn clock_mut(&mut self, colour: Colour) -> &mut SideClock {
        &mut self.clocks[colour.index()]
    }

    /// Sets `parameter` to `value`.
    ///
    /// # Errors
    ///
    /// Returns [`GoError::InvalidValue`] if `value` is missing or invalid, leaving the
    /// request unchanged.
    fn set(&mut self, parameter: Parameter, value: &str) -> Result<(), GoError> {
        let name = parameter.name();
        match parameter {
            Parameter::Depth => self.limits.depth = Some(parse_count(name, value)?),
            Parameter::Nodes => self.limits.nodes = Some(parse_count(name, value)?),
            Parameter::MoveTime => self.movetime = Some(parse_millis(name, value)?),
            Parameter::WhiteTime => {
                self.clock_mut(Colour::White).time = Some(parse_millis(name, value)?);
            }
            Parameter::BlackTime => {
                self.clock_mut(Colour::Black).time = Some(parse_millis(name, value)?);
            }
            Parameter::WhiteIncrement => {
                self.clock_mut(Colour::White).increment = Some(parse_millis(name, value)?);
            }
            Parameter::BlackIncrement => {
                self.clock_mut(Colour::Black).increment = Some(parse_millis(name, value)?);
            }
            Parameter::MovesToGo => {
                let moves = parse_count(name, value).ok().filter(|&n| n > 0);
                self.moves_to_go = Some(moves.ok_or_else(|| GoError::InvalidValue {
                    parameter: name,
                    value: value.to_owned(),
                })?);
            }
        }
        Ok(())
    }

    /// The plan for a search with `side` to move, keeping `overhead` back from the time to
    /// move.
    ///
    /// `movetime` takes precedence over the clock. Without either, the search has no time
    /// limit. A search that nothing would end (no depth, nodes or time, where a clock for
    /// the other side alone does not count) is infinite, as if `infinite` were given.
    pub fn plan(&self, side: Colour, overhead: Duration) -> SearchPlan {
        let SideClock { time, increment } = self.clocks[side.index()];
        let clock = time.map(|time| Clock {
            time,
            increment: increment.unwrap_or_default(),
            moves_to_go: self.moves_to_go,
        });
        let time = match (self.movetime, clock) {
            (Some(movetime), _) => Some(TimeLimit::movetime(movetime, overhead)),
            (None, Some(clock)) => Some(TimeLimit::clock(clock, overhead)),
            (None, None) => None,
        };
        let limits = Limits {
            time,
            ..self.limits
        };
        SearchPlan {
            limits,
            infinite: self.infinite || limits == Limits::default(),
        }
    }
}

/// How a search should run, worked out from a [`SearchRequest`] for the side to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SearchPlan {
    /// When the search stops.
    pub limits: Limits,
    /// Whether the `bestmove` must wait for `stop` even if the search ends sooner.
    pub infinite: bool,
}

/// Parses a `go` command.
///
/// A search request is parsed leniently: an unsupported parameter, or one with a missing
/// or invalid value, is dropped and listed in [`Go::Search::ignored`], and parsing carries
/// on with the next word.
///
/// # Errors
///
/// Returns an error if `go perft` is not followed by exactly one valid depth.
pub(super) fn parse_go(line: &str) -> Result<Go, GoError> {
    let args: Vec<&str> = line.split_whitespace().skip(1).collect();
    if let ["perft", rest @ ..] = args.as_slice() {
        return match rest {
            [depth] => depth
                .parse()
                .map(Go::Perft)
                .map_err(|_| GoError::InvalidValue {
                    parameter: "perft depth",
                    value: (*depth).to_owned(),
                }),
            _ => Err(GoError::MalformedPerft),
        };
    }
    let mut request = SearchRequest::default();
    let mut ignored = Vec::new();
    let mut args = args.into_iter().peekable();
    while let Some(name) = args.next() {
        if name == "infinite" {
            request.infinite = true;
        } else if let Some(parameter) = Parameter::from_name(name) {
            // With no value before the next parameter, the value is missing.
            let value = args
                .next_if(|&next| !is_parameter(next))
                .unwrap_or_default();
            if let Err(err) = request.set(parameter, value) {
                ignored.push(err);
            }
        } else {
            ignored.push(GoError::Unsupported(name.to_owned()));
        }
    }
    Ok(Go::Search { request, ignored })
}

/// A `go` parameter this engine supports that takes a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parameter {
    Depth,
    Nodes,
    MoveTime,
    WhiteTime,
    BlackTime,
    WhiteIncrement,
    BlackIncrement,
    MovesToGo,
}

impl Parameter {
    /// Every parameter, for looking one up by name.
    const ALL: [Self; 8] = [
        Self::Depth,
        Self::Nodes,
        Self::MoveTime,
        Self::WhiteTime,
        Self::BlackTime,
        Self::WhiteIncrement,
        Self::BlackIncrement,
        Self::MovesToGo,
    ];

    /// The parameter named `name`, if this engine supports it.
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|parameter| parameter.name() == name)
    }

    /// The parameter's name in a `go` command.
    const fn name(self) -> &'static str {
        match self {
            Self::Depth => "depth",
            Self::Nodes => "nodes",
            Self::MoveTime => "movetime",
            Self::WhiteTime => "wtime",
            Self::BlackTime => "btime",
            Self::WhiteIncrement => "winc",
            Self::BlackIncrement => "binc",
            Self::MovesToGo => "movestogo",
        }
    }
}

/// Reports whether `word` names a `go` parameter this engine supports.
fn is_parameter(word: &str) -> bool {
    word == "infinite" || Parameter::from_name(word).is_some()
}

/// Parses `value`, the value given for the `go` parameter `parameter`, as a count.
fn parse_count<T: FromStr>(parameter: &'static str, value: &str) -> Result<T, GoError> {
    value.parse().map_err(|_| GoError::InvalidValue {
        parameter,
        value: value.to_owned(),
    })
}

/// Parses `value`, the value given for the `go` parameter `parameter`, as a time in
/// milliseconds.
fn parse_millis(parameter: &'static str, value: &str) -> Result<Duration, GoError> {
    parse_count(parameter, value).map(Duration::from_millis)
}

/// Why a `go` command was rejected.
#[derive(Debug)]
pub(super) enum GoError {
    /// `perft` was not followed by exactly one depth.
    MalformedPerft,
    /// A parameter's value was missing or not a valid count.
    InvalidValue {
        /// The parameter whose value was rejected.
        parameter: &'static str,
        /// The value given, or an empty string if there was none.
        value: String,
    },
    /// The parameter is not supported.
    Unsupported(String),
}

impl fmt::Display for GoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedPerft => f.write_str("expected go perft <depth>"),
            Self::InvalidValue { parameter, value } => {
                write!(f, "invalid {parameter} {value:?}")
            }
            Self::Unsupported(name) => write!(f, "unsupported parameter {name:?}"),
        }
    }
}
