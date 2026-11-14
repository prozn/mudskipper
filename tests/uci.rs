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
    assert!(engine.wait_exit().success());
}

#[test]
fn unknown_command_gets_info_string_and_engine_keeps_running() {
    let mut engine = Engine::start();
    engine.send("frobnicate the board");
    let (reply, _) = engine.read_until("info string", |l| l.starts_with("info string"));
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
    engine.read_until("info string", |l| l.starts_with("info string"));
    assert!(engine.is_running(), "engine exited after a malformed line");
    engine.send("isready");
    engine.expect_line("readyok");
}
