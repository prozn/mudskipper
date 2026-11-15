//! UCI protocol behaviour of the engine binary (Seam 1).

mod common;

use common::{Engine, Reply};

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

#[test]
fn uci_lists_the_uci_chess960_option() {
    let mut engine = Engine::start();
    engine.send("uci");
    let before = engine.expect_line("uciok");
    assert!(
        before
            .iter()
            .any(|l| l == "option name UCI_Chess960 type check default false"),
        "missing UCI_Chess960 option in {before:?}"
    );
}

/// Sends `go perft 1` and returns the moves it lists.
fn perft_moves(engine: &mut Engine) -> Vec<String> {
    engine.send("go perft 1");
    let reply = engine.read_line_matching("a Nodes searched line", |line| {
        line.starts_with("Nodes searched: ")
    });
    reply
        .before
        .iter()
        .filter_map(|line| line.split_once(':').map(|(mv, _)| mv.to_owned()))
        .collect()
}

#[test]
fn uci_chess960_toggles_castling_between_king_takes_rook_and_standard_notation() {
    let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    let castled = "2kr3r/8/8/8/8/8/8/R4RK1 w - - 2 2";
    let mut engine = Engine::start();

    engine.send("setoption name UCI_Chess960 value true");
    engine.send(&format!("position fen {fen}"));
    let moves = perft_moves(&mut engine);
    for castling in ["e1h1", "e1a1"] {
        assert!(
            moves.iter().any(|mv| mv == castling),
            "{castling} missing from {moves:?}"
        );
    }
    for standard in ["e1g1", "e1c1"] {
        assert!(
            !moves.iter().any(|mv| mv == standard),
            "{standard} listed in {moves:?}"
        );
    }
    engine.send(&format!("position fen {fen} moves e1h1 e8a8"));
    assert_eq!(reported_fen(&mut engine), castled);
    engine.send(&format!("position fen {fen} moves e1g1"));
    let reply = engine.expect_info_string();
    assert!(reply.contains("e1g1"), "e1g1 should be illegal: {reply:?}");

    engine.send("setoption name UCI_Chess960 value false");
    engine.send(&format!("position fen {fen}"));
    let moves = perft_moves(&mut engine);
    for standard in ["e1g1", "e1c1"] {
        assert!(
            moves.iter().any(|mv| mv == standard),
            "{standard} missing from {moves:?}"
        );
    }
    engine.send(&format!("position fen {fen} moves e1g1 e8c8"));
    assert_eq!(reported_fen(&mut engine), castled);
}

#[test]
fn uci_chess960_d_reports_shredder_fen() {
    let mut engine = Engine::start();
    engine.send("setoption name UCI_Chess960 value true");
    engine.send("position startpos");
    assert_eq!(
        reported_fen(&mut engine),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w HAha - 0 1"
    );
}

#[test]
fn malformed_setoption_commands_get_info_strings() {
    let mut engine = Engine::start();
    for command in [
        "setoption",
        "setoption name Hash value 16",
        "setoption name UCI_Chess960 value maybe",
        "setoption name UCI_Chess960",
    ] {
        engine.send(command);
        let reply = engine.expect_info_string();
        assert!(
            reply.contains("invalid setoption"),
            "{command:?} should be reported as a bad setoption command: {reply:?}"
        );
    }
    engine.send("isready");
    engine.expect_line("readyok");
}

/// Sets up `fen`, sends `eval` and returns the white-relative score it prints.
fn eval_score(engine: &mut Engine, fen: &str) -> i32 {
    engine.send(&format!("position fen {fen}"));
    engine.send("eval");
    let reply = engine.read_line_matching("an Evaluation: line", |line| {
        line.starts_with("Evaluation: ")
    });
    let rest = &reply.line["Evaluation: ".len()..];
    let score = rest.strip_suffix(" (white side)").unwrap_or_else(|| {
        panic!(
            "eval should say whose side the score is from: {:?}",
            reply.line
        )
    });
    score
        .parse()
        .unwrap_or_else(|_| panic!("eval should print an integer score: {:?}", reply.line))
}

#[test]
fn eval_of_the_start_position_is_small() {
    let mut engine = Engine::start();
    let score = eval_score(
        &mut engine,
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    );
    assert!(
        score.abs() < 50,
        "start position should be near equal: {score}"
    );
}

#[test]
fn eval_favours_the_side_with_extra_material() {
    let mut engine = Engine::start();
    let score = eval_score(&mut engine, "4k3/8/8/8/8/8/8/3QK3 w - - 0 1");
    assert!(
        score > 500,
        "an extra queen should score well for white: {score}"
    );
}

#[test]
fn eval_of_mirrored_positions_is_equal_and_opposite() {
    // Each pair is a position and its colour-flipped mirror image.
    let pairs = [
        (
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "r3k2r/pppbbppp/2n2q1P/1P2p3/3pn3/BN2PNP1/P1PPQPB1/R3K2R b KQkq - 0 1",
        ),
        (
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "8/4p1p1/8/1r3P1K/kp5R/3P4/2P5/8 b - - 0 1",
        ),
        (
            "rnbqkb1r/pp1p1ppp/4pn2/2p5/2PP4/2N5/PP2PPPP/R1BQKBNR w KQkq - 0 4",
            "r1bqkbnr/pp2pppp/2n5/2pp4/2P5/4PN2/PP1P1PPP/RNBQKB1R b KQkq - 0 4",
        ),
    ];
    let mut engine = Engine::start();
    for (fen, mirrored) in pairs {
        let score = eval_score(&mut engine, fen);
        let mirrored_score = eval_score(&mut engine, mirrored);
        assert_ne!(score, 0, "{fen} should not evaluate to exactly zero");
        assert_eq!(
            mirrored_score, -score,
            "{mirrored} should score the opposite of {fen}"
        );
    }
}

/// Sends `go` with `limits` and returns the `bestmove` reply with the lines before it.
fn search(engine: &mut Engine, limits: &str) -> Reply {
    engine.send(&format!("go {limits}"));
    engine.read_line_matching("a bestmove line", |line| line.starts_with("bestmove "))
}

/// The move named by a `bestmove` line.
fn best_move(reply: &Reply) -> &str {
    let mut words = reply.line.split_whitespace().skip(1);
    words
        .next()
        .unwrap_or_else(|| panic!("bestmove line names no move: {:?}", reply.line))
}

/// The search depth lines: `info` lines carrying a `depth`, as opposed to `info string`.
fn depth_lines(reply: &Reply) -> Vec<&str> {
    reply
        .before
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with("info depth "))
        .collect()
}

/// The value following `field` in an `info` line, or `None` if the field is absent.
fn info_field<'line>(line: &'line str, field: &str) -> Option<&'line str> {
    let mut words = line.split_whitespace();
    words.find(|&word| word == field)?;
    words.next()
}

/// Checks that an `info` line carries every field the spec asks for, well formed, and
/// returns its principal variation.
fn check_info_line(line: &str) -> Vec<&str> {
    for field in ["depth", "seldepth", "nodes", "nps", "time", "hashfull"] {
        let value =
            info_field(line, field).unwrap_or_else(|| panic!("info line lacks {field}: {line:?}"));
        assert!(
            value.parse::<u64>().is_ok(),
            "{field} should be a count: {line:?}"
        );
    }
    let kind =
        info_field(line, "score").unwrap_or_else(|| panic!("info line lacks score: {line:?}"));
    let value = info_field(line, kind).expect("score kind is followed by a value");
    assert!(
        matches!(kind, "cp" | "mate") && value.parse::<i32>().is_ok(),
        "score should be cp or mate with an integer: {line:?}"
    );
    let (_, pv) = line
        .split_once(" pv ")
        .unwrap_or_else(|| panic!("info line lacks pv: {line:?}"));
    pv.split_whitespace().collect()
}

#[test]
fn go_depth_returns_a_legal_bestmove_with_well_formed_info_lines() {
    let positions = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    ];
    let mut engine = Engine::start();
    for fen in positions {
        engine.send(&format!("position fen {fen}"));
        let legal = perft_moves(&mut engine);
        let reply = search(&mut engine, "depth 3");
        let best = best_move(&reply);
        assert!(
            legal.iter().any(|mv| mv == best),
            "bestmove {best} is not legal in {fen}"
        );
        let lines = depth_lines(&reply);
        assert!(
            !lines.is_empty(),
            "no info lines for {fen}: {:?}",
            reply.before
        );
        for line in &lines {
            let pv = check_info_line(line);
            assert!(!pv.is_empty(), "pv should not be empty: {line:?}");
        }
        let last = lines.last().expect("lines is not empty");
        assert_eq!(
            info_field(last, "depth"),
            Some("3"),
            "last info line should report the requested depth for {fen}"
        );
        assert_eq!(
            check_info_line(last).first(),
            Some(&best),
            "pv should start with the best move for {fen}"
        );
    }
}

/// The score reported by the last search depth line, such as `cp 31` or `mate 2`.
fn final_score(reply: &Reply) -> String {
    let last = depth_lines(reply)
        .pop()
        .unwrap_or_else(|| panic!("no info lines before {:?}", reply.line));
    let kind = info_field(last, "score").unwrap_or_else(|| panic!("no score in {last:?}"));
    let value = info_field(last, kind).expect("score kind is followed by a value");
    format!("{kind} {value}")
}

#[test]
fn go_depth_finds_mate_in_one_and_mate_in_two() {
    // Each position has exactly one mating first move.
    let cases = [
        ("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", "a1a8", "mate 1"),
        ("r5k1/5ppp/8/8/8/8/1R6/1R4K1 w - - 0 1", "b2b8", "mate 2"),
    ];
    let mut engine = Engine::start();
    for (fen, mating_move, score) in cases {
        engine.send(&format!("position fen {fen}"));
        let reply = search(&mut engine, "depth 4");
        assert_eq!(best_move(&reply), mating_move, "best move in {fen}");
        assert_eq!(final_score(&reply), score, "score in {fen}");
    }
}

#[test]
fn go_depth_reports_being_mated_as_a_negative_mate_score() {
    let mut engine = Engine::start();
    engine.send("position fen r5k1/5ppp/8/8/8/8/1R6/1R4K1 w - - 0 1 moves b2b8");
    let reply = search(&mut engine, "depth 4");
    assert_eq!(best_move(&reply), "a8b8", "the only legal move");
    assert_eq!(final_score(&reply), "mate -1");
}

#[test]
fn go_depth_without_legal_moves_returns_bestmove_0000() {
    let mut engine = Engine::start();
    for position in [
        // Fool's mate: White is checkmated.
        "startpos moves f2f3 e7e5 g2g4 d8h4",
        // Black is stalemated.
        "fen 7k/5Q2/6K1/8/8/8/8/8 b - - 0 1",
    ] {
        engine.send(&format!("position {position}"));
        let reply = search(&mut engine, "depth 3");
        assert_eq!(reply.line, "bestmove 0000", "after position {position}");
    }
}

#[test]
fn go_nodes_stops_near_the_node_limit_with_a_legal_bestmove() {
    let mut engine = Engine::start();
    let legal = perft_moves(&mut engine);
    for limit in [1_u64, 20_000, 100_000] {
        let reply = search(&mut engine, &format!("nodes {limit}"));
        let best = best_move(&reply);
        assert!(
            legal.iter().any(|mv| mv == best),
            "bestmove {best} is not legal with nodes {limit}"
        );
        let Some(last) = depth_lines(&reply).pop() else {
            assert_eq!(limit, 1, "only a tiny limit may end before any info line");
            continue;
        };
        let nodes: u64 = info_field(last, "nodes")
            .and_then(|nodes| nodes.parse().ok())
            .unwrap_or_else(|| panic!("no node count in {last:?}"));
        assert!(
            nodes.abs_diff(limit) <= limit / 100,
            "searched {nodes} nodes with a limit of {limit}"
        );
    }
}

#[test]
fn malformed_go_commands_get_info_strings() {
    let mut engine = Engine::start();
    for command in [
        "go",
        "go depth",
        "go depth x",
        "go nodes -5",
        "go depth 3 sideways",
    ] {
        engine.send(command);
        let reply = engine.expect_info_string();
        assert!(
            reply.contains("invalid go"),
            "{command:?} should be reported as a bad go command: {reply:?}"
        );
    }
    engine.send("isready");
    engine.expect_line("readyok");
}
