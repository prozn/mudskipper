//! Criterion benchmarks for move generation, making moves and perft (Seam 2).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mudskipper::chess::Board;

const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

fn kiwipete() -> Board {
    Board::from_fen(KIWIPETE).expect("Kiwipete FEN is valid")
}

fn movegen(c: &mut Criterion) {
    let board = kiwipete();
    c.bench_function("pseudo_legal_moves kiwipete", |b| {
        b.iter(|| black_box(&board).pseudo_legal_moves());
    });
    c.bench_function("noisy_moves kiwipete", |b| {
        b.iter(|| black_box(&board).noisy_moves());
    });
    c.bench_function("legal_moves kiwipete", |b| {
        b.iter(|| black_box(&board).legal_moves());
    });
}

fn make(c: &mut Criterion) {
    let board = kiwipete();
    let moves = board.legal_moves();
    c.bench_function("make every legal move kiwipete", |b| {
        b.iter(|| {
            for &mv in &moves {
                black_box(black_box(&board).make(mv));
            }
        });
    });
}

fn perft(c: &mut Criterion) {
    let startpos = Board::startpos();
    let kiwipete = kiwipete();
    let mut group = c.benchmark_group("perft");
    group.sample_size(10);
    group.bench_function("startpos depth 4", |b| {
        b.iter(|| black_box(&startpos).perft(black_box(4)));
    });
    group.bench_function("kiwipete depth 3", |b| {
        b.iter(|| black_box(&kiwipete).perft(black_box(3)));
    });
    group.finish();
}

criterion_group!(benches, movegen, make, perft);
criterion_main!(benches);
