//! Datagen's viriformat output, checked against the `viriformat` crate that bullet reads it
//! with.

#![cfg(feature = "datagen")]

use std::io::BufReader;
use std::sync::atomic::Ordering;

use mudskipper::chess::{Board, CastlingNotation};
use mudskipper::search::TranspositionTable;
use mudskipper::tools::datagen::{self, GameRecord, Outcome, Rng, Settings};
use viriformat::dataformat::{Game, WDL};

fn parse(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}"))
}

/// Records `moves`, written in UCI notation with castling as king-takes-rook, from
/// `board`, each with its white-relative score.
fn record(board: &Board, moves: &[(&str, i16)]) -> GameRecord {
    let mut game = GameRecord::new(board.clone());
    let mut board = board.clone();
    for &(text, score) in moves {
        let mv = board
            .parse_move(text, CastlingNotation::KingTakesRook)
            .unwrap_or_else(|err| panic!("{text} in {}: {err}", board.to_fen()));
        game.push(mv, score);
        board = board.make(mv);
    }
    game
}

fn write(game: &GameRecord, outcome: Outcome) -> Vec<u8> {
    let mut bytes = Vec::new();
    game.write(outcome, &mut bytes)
        .expect("writing to a Vec cannot fail");
    bytes
}

/// Decodes one game, which must fill `bytes`.
///
/// In a debug build the crate checks each move is legal. Its Chess960 castling rules,
/// which cover standard castling too, are a global switch, turned on here.
fn decode(bytes: &[u8]) -> Game {
    viriformat::chess::CHESS960.store(true, Ordering::Relaxed);
    let mut reader = BufReader::new(bytes);
    let game = Game::deserialise_from(&mut reader, Vec::new())
        .unwrap_or_else(|err| panic!("viriformat rejects {bytes:02x?}: {err}"));
    assert!(reader.buffer().is_empty(), "bytes left after the game");
    game
}

/// The decoded moves in UCI notation with castling as king-takes-rook, each with its
/// score and whether the crate reads it as castling.
fn moves(game: &Game) -> Vec<(String, i16, bool)> {
    game.moves
        .iter()
        .map(|(mv, score)| (mv.display(true).to_string(), score.get(), mv.is_castle()))
        .collect()
}

/// The README's example game: 1. e4 e5 2. Qh5 Ke7 3. Qxe5#.
///
/// The bytes are those of the README on GitHub from April 2026 on. The copy bundled with
/// the 2.0.1 crate prints the piece nibbles high-first (`6124 5216 …`), which is wrong:
/// the crate, and so bullet, stores entry `i` in the low nibble of byte `i / 2` for even
/// `i`.
#[test]
fn writes_the_readme_example_game() {
    let game = record(
        &Board::startpos(),
        &[
            ("e2e4", 10),
            ("e7e5", 20),
            ("d1h5", -30),
            ("e8e7", 32767),
            ("h5e5", 32767),
        ],
    );
    #[rustfmt::skip]
    let expected: [u8; 56] = [
        0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, // occupancy
        0x16, 0x42, 0x25, 0x61, 0x00, 0x00, 0x00, 0x00, // pieces
        0x88, 0x88, 0x88, 0x88, 0x9e, 0xca, 0xad, 0xe9,
        0x40, // white to move, no en passant
        0x00, // halfmove clock
        0x01, 0x00, // fullmove counter
        0x00, 0x00, // position score
        0x02, // white win
        0x00, // extra byte
        0x0c, 0x07, 0x0a, 0x00, // 1. e4      +10
        0x34, 0x09, 0x14, 0x00, // 1. ... e5  +20
        0xc3, 0x09, 0xe2, 0xff, // 2. Qh5     -30
        0x3c, 0x0d, 0xff, 0x7f, // 2. ... Ke7 +32767
        0x27, 0x09, 0xff, 0x7f, // 3. Qxe5#   +32767
        0x00, 0x00, 0x00, 0x00, // terminator
    ];
    let bytes = write(&game, Outcome::WhiteWin);
    assert_eq!(bytes, expected);

    let decoded = decode(&bytes);
    assert_eq!(decoded.outcome(), WDL::Win);
    assert_eq!(
        moves(&decoded),
        [
            ("e2e4".to_owned(), 10, false),
            ("e7e5".to_owned(), 20, false),
            ("d1h5".to_owned(), -30, false),
            ("e8e7".to_owned(), 32767, false),
            ("h5e5".to_owned(), 32767, false),
        ],
    );
}

/// Both sides castle in a Chess960 position whose castling rooks stand on the b- and
/// e-files: White on the queen side, where the king stays on c1, and Black on the king
/// side. The crate must read the unmoved rooks as castling rights, or, in a debug build,
/// it rejects the castling moves as illegal.
#[test]
fn chess960_castling_survives_the_round_trip() {
    let start = parse("nrk1r3/pppppppp/8/8/8/8/PPPPPPPP/NRK1R3 w EBeb - 0 1");
    let game = record(&start, &[("c1b1", 15), ("c8e8", -5)]);
    let decoded = decode(&write(&game, Outcome::Draw));
    assert_eq!(decoded.outcome(), WDL::Draw);
    assert_eq!(
        moves(&decoded),
        [("c1b1".to_owned(), 15, true), ("c8e8".to_owned(), -5, true)],
    );
}

/// A short self-play game, played with small node limits so that it is quick in a debug
/// build, decodes to the moves, scores and result that were recorded.
#[test]
fn a_generated_game_decodes_to_what_was_played() {
    let settings = Settings {
        soft_nodes: 200,
        hard_nodes: 1_600,
        opening_nodes: 1_000,
    };
    let tt = TranspositionTable::new(1);
    let mut rng = Rng::new(0x5eed);
    let (game, outcome) = std::iter::repeat_with(|| datagen::play_game(&mut rng, &settings, &tt))
        .take(100)
        .flatten()
        .next()
        .expect("one of 100 openings is playable");
    assert!(
        !game.is_empty(),
        "a game is recorded from the end of its opening"
    );

    let decoded = decode(&write(&game, outcome));
    let played: Vec<(String, i16, bool)> = game
        .moves()
        .iter()
        .map(|&(mv, score)| {
            let text = mv.display(CastlingNotation::KingTakesRook).to_string();
            (text, score, mv.is_castling())
        })
        .collect();
    assert_eq!(moves(&decoded), played);
    let wdl = match outcome {
        Outcome::WhiteWin => WDL::Win,
        Outcome::Draw => WDL::Draw,
        Outcome::BlackWin => WDL::Loss,
    };
    assert_eq!(decoded.outcome(), wdl);
}

/// With one castling right per colour left, only that rook is written as unmoved: the
/// crate reads back exactly those rights, which its FEN writes by side, as `Q` for
/// White's b-rook and `k` for Black's e-rook.
#[test]
fn only_rooks_that_may_castle_are_unmoved() {
    let start = parse("nrk1r3/pppppppp/8/8/8/8/PPPPPPPP/NRK1R3 w Be - 0 1");
    let decoded = decode(&write(&GameRecord::new(start), Outcome::Draw));
    let fen = decoded.initial_position().to_string();
    let castling = fen.split(' ').nth(2);
    assert_eq!(castling, Some("Qk"), "decoded as {fen}");
}
