//! UCI protocol behaviour of the engine binary (Seam 1).

mod common;

use common::Engine;

#[test]
fn uci_handshake_reports_id_then_uciok() {
    let mut engine = Engine::start();
    engine.send("uci");
    let before = engine.expect_line("uciok");
    assert!(
        before.iter().any(|l| l.starts_with("id name Mudskipper")),
        "missing id name in {before:?}"
    );
    assert!(
        before.iter().any(|l| l.starts_with("id author ")),
        "missing id author in {before:?}"
    );
}

#[test]
fn isready_is_answered_with_readyok() {
    let mut engine = Engine::start();
    engine.send("isready");
    engine.expect_line("readyok");
}

#[test]
fn quit_exits_cleanly_without_closing_stdin() {
    let mut engine = Engine::start();
    engine.send("quit");
    let status = engine.wait_exit();
    assert!(status.success(), "engine exited with {status}");
}

#[test]
fn crlf_line_endings_work_like_lf() {
    let mut engine = Engine::start();
    engine.send_raw("uci\r\n");
    engine.expect_line("uciok");
    engine.send_raw("isready\r\n");
    engine.expect_line("readyok");
    engine.send_raw("quit\r\n");
    let status = engine.wait_exit();
    assert!(status.success(), "engine exited with {status}");
}

#[test]
fn unknown_command_gets_info_string_and_engine_keeps_running() {
    let mut engine = Engine::start();
    engine.send("frobnicate the board");
    let reply = engine.expect_info_string();
    assert!(
        reply.contains("frobnicate"),
        "reply should name the command: {reply:?}"
    );
    assert!(
        engine.is_running(),
        "engine exited after an unknown command"
    );
    engine.send("isready");
    engine.expect_line("readyok");
}

#[test]
fn invalid_utf8_line_gets_info_string_and_engine_keeps_running() {
    let mut engine = Engine::start();
    engine.send_bytes(b"position \xff\xfe startpos\n");
    engine.expect_info_string();
    assert!(engine.is_running(), "engine exited after a malformed line");
    engine.send("isready");
    engine.expect_line("readyok");
}

/// Sends `d` and returns the FEN it reports.
fn reported_fen(engine: &mut Engine) -> String {
    engine.send("d");
    let reply = engine.read_line_matching("a Fen: line", |line| line.starts_with("Fen: "));
    reply.line["Fen: ".len()..].to_owned()
}

#[test]
fn position_fen_then_d_reports_that_fen() {
    let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    let mut engine = Engine::start();
    engine.send(&format!("position fen {fen}"));
    assert_eq!(reported_fen(&mut engine), fen);
}

#[test]
fn d_prints_the_board_and_a_zobrist_key() {
    let mut engine = Engine::start();
    engine.send("position fen 4k3/8/8/8/8/8/8/R3K3 w Q - 0 1");
    engine.send("d");
    let reply = engine.read_line_matching("a Key: line", |line| line.starts_with("Key: "));
    let key = &reply.line["Key: ".len()..];
    assert!(
        key.len() == 16 && key.chars().all(|c| c.is_ascii_hexdigit()),
        "key should be 16 hex digits: {key:?}"
    );
    assert!(
        reply.before.iter().any(|l| l.contains("| k |")),
        "board should show the black king on the eighth rank: {:?}",
        reply.before
    );
}

#[test]
fn position_startpos_then_d_reports_the_start_fen() {
    let mut engine = Engine::start();
    engine.send("position fen 4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    engine.send("position startpos");
    assert_eq!(
        reported_fen(&mut engine),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}

#[test]
fn engine_starts_in_the_start_position() {
    let mut engine = Engine::start();
    assert_eq!(
        reported_fen(&mut engine),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}

#[test]
fn malformed_fen_gets_info_string_and_keeps_the_previous_position() {
    let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
    let mut engine = Engine::start();
    engine.send(&format!("position fen {fen}"));
    engine.send("position fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1");
    let reply = engine.expect_info_string();
    assert!(
        reply.contains("side to move"),
        "reply should explain the error: {reply:?}"
    );
    assert!(engine.is_running(), "engine exited after a malformed FEN");
    assert_eq!(reported_fen(&mut engine), fen);
}

#[test]
fn malformed_position_commands_get_info_strings() {
    let mut engine = Engine::start();
    for command in [
        "position",
        "position fen",
        "position somewhere",
        "position startpos extra",
        "position startpos moves e2e5",
        "position startpos moves e2e4 e2e4",
        "position startpos moves e1g1",
        "position startpos moves e2",
    ] {
        engine.send(command);
        let reply = engine.expect_info_string();
        assert!(
            !reply.contains("unknown command"),
            "{command:?} should be reported as a bad position command: {reply:?}"
        );
    }
    assert!(
        engine.is_running(),
        "engine exited after a malformed command"
    );
}

#[test]
fn position_with_moves_then_go_perft_reports_the_total() {
    let mut engine = Engine::start();
    engine.send("position startpos moves e2e4 e7e5");
    engine.send("go perft 3");
    // Reference counts from python-chess.
    let before = engine.expect_line("Nodes searched: 24825");
    assert!(
        before.iter().any(|line| line == "g1f3: 779"),
        "split counts should list each root move: {before:?}"
    );
}

#[test]
fn position_with_moves_then_d_reports_the_resulting_fen() {
    let mut engine = Engine::start();
    engine.send("position startpos moves e2e4 c7c5 g1f3");
    assert_eq!(
        reported_fen(&mut engine),
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2"
    );
}

#[test]
fn castling_and_promotion_moves_are_read_in_standard_notation() {
    let mut engine = Engine::start();
    engine.send("position fen r3k2r/1P6/8/8/8/8/8/R3K2R w KQkq - 0 1 moves e1c1 e8g8 b7a8n");
    // Expected FEN from python-chess.
    assert_eq!(
        reported_fen(&mut engine),
        "N4rk1/8/8/8/8/8/8/2KR3R b - - 0 2"
    );
}

#[test]
fn illegal_move_keeps_the_previous_position() {
    let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
    let mut engine = Engine::start();
    engine.send(&format!("position fen {fen}"));
    engine.send("position startpos moves e2e4 e7e5 e1e3");
    let reply = engine.expect_info_string();
    assert!(
        reply.contains("e1e3"),
        "reply should name the move: {reply:?}"
    );
    assert_eq!(reported_fen(&mut engine), fen);
}

#[test]
fn go_perft_zero_counts_the_position_itself() {
    let mut engine = Engine::start();
    engine.send("go perft 0");
    engine.expect_line("Nodes searched: 1");
}

#[test]
fn go_perft_in_checkmate_counts_no_nodes() {
    let mut engine = Engine::start();
    engine.send("position startpos moves f2f3 e7e5 g2g4 d8h4");
    engine.send("go perft 1");
    engine.expect_line("Nodes searched: 0");
}

#[test]
fn malformed_go_perft_gets_an_info_string() {
    let mut engine = Engine::start();
    for command in ["go perft", "go perft x", "go perft -1"] {
        engine.send(command);
        let reply = engine.expect_info_string();
        assert!(
            reply.contains("perft"),
            "{command:?} should be reported as a bad perft command: {reply:?}"
        );
    }
    engine.send("isready");
    engine.expect_line("readyok");
}
