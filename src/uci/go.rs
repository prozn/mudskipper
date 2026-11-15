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
    Search(SearchRequest),
}

/// A `go` command that asks for a search.
#[derive(Debug, Default)]
pub(super) struct SearchRequest {
    /// The depth and node limits.
    pub limits: Limits,
    /// The time to search for, from `movetime`.
    pub movetime: Option<Duration>,
    /// The time each side has left, from `wtime` and `btime`, indexed by colour.
    pub time: [Option<Duration>; 2],
    /// The time each side gains after each move, from `winc` and `binc`, indexed by
    /// colour.
    pub increment: [Option<Duration>; 2],
    /// The moves left until the next time control, from `movestogo`.
    pub moves_to_go: Option<u32>,
    /// Whether `infinite` was given: the `bestmove` must wait for `stop` even if the
    /// search ends sooner.
    pub infinite: bool,
}

impl SearchRequest {
    /// The limits of a search with `side` to move, keeping `overhead` back from the time
    /// to move.
    ///
    /// `movetime` takes precedence over the clock. Without either, the search has no time
    /// limit.
    pub fn limits(&self, side: Colour, overhead: Duration) -> Limits {
        let clock = self.time[side.index()].map(|time| Clock {
            time,
            increment: self.increment[side.index()].unwrap_or_default(),
            moves_to_go: self.moves_to_go,
        });
        let time = match (self.movetime, clock) {
            (Some(movetime), _) => Some(TimeLimit::movetime(movetime, overhead)),
            (None, Some(clock)) => Some(TimeLimit::clock(clock, overhead)),
            (None, None) => None,
        };
        Limits {
            time,
            ..self.limits
        }
    }

    /// Reports whether anything says when the search should end.
    fn has_limit(&self) -> bool {
        self.limits != Limits::default()
            || self.movetime.is_some()
            || self.time.iter().any(Option::is_some)
            || self.infinite
    }
}

/// Parses a `go` command.
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
    let mut args = args.into_iter();
    while let Some(name) = args.next() {
        if name == "infinite" {
            request.infinite = true;
            continue;
        }
        let value = args.next().unwrap_or_default();
        match name {
            "depth" => request.limits.depth = Some(parse_count("depth", value)?),
            "nodes" => request.limits.nodes = Some(parse_count("nodes", value)?),
            "movetime" => request.movetime = Some(parse_millis("movetime", value)?),
            "wtime" => request.time[Colour::White.index()] = Some(parse_millis("wtime", value)?),
            "btime" => request.time[Colour::Black.index()] = Some(parse_millis("btime", value)?),
            "winc" => {
                request.increment[Colour::White.index()] = Some(parse_millis("winc", value)?);
            }
            "binc" => {
                request.increment[Colour::Black.index()] = Some(parse_millis("binc", value)?);
            }
            "movestogo" => {
                let moves = parse_count("movestogo", value).ok().filter(|&n| n > 0);
                request.moves_to_go = Some(moves.ok_or_else(|| GoError::InvalidValue {
                    parameter: "movestogo",
                    value: value.to_owned(),
                })?);
            }
            other => return Err(GoError::Unsupported(other.to_owned())),
        }
    }
    if !request.has_limit() {
        return Err(GoError::MissingLimit);
    }
    Ok(Go::Search(request))
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
    /// Nothing said when the search should end.
    MissingLimit,
}

impl fmt::Display for GoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedPerft => f.write_str("expected go perft <depth>"),
            Self::InvalidValue { parameter, value } => {
                write!(f, "invalid {parameter} {value:?}")
            }
            Self::Unsupported(name) => write!(f, "unsupported parameter {name:?}"),
            Self::MissingLimit => {
                f.write_str("expected depth, nodes, movetime, wtime, btime or infinite")
            }
        }
    }
}
