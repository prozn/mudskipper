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
