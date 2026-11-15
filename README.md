# Mudskipper

Mudskipper is a UCI chess engine written in Rust. It speaks the
[Universal Chess Interface](https://backscattering.de/chess/uci/), so it runs inside any
UCI GUI (Cute Chess, Arena, Banksia, En Croissant) or match runner (fastchess,
cutechess-cli, OpenBench).

It is at an early stage. It sets up positions with `position startpos` or
`position fen <fen>`, optionally followed by `moves ...`, and searches them with `go`,
replying with `info` lines and a `bestmove`. `go` accepts `depth`, `nodes`, `movetime`,
`infinite`, and the clock parameters `wtime`, `btime`, `winc`, `binc` and `movestogo`.
Commands are read on their own thread, so `stop` and `isready` are answered during a
search; other commands sent during a search wait until it ends. The search is a plain
alpha-beta search with quiescence search over a PeSTO-style tapered evaluation.

It offers these UCI options:

| Option | Type | Default | Effect |
|---|---|---|---|
| `Hash` | spin, 1 to 65536 | 16 | Transposition table size in MB. Accepted, but there is no transposition table yet. |
| `Threads` | spin, 1 to 1024 | 1 | Search threads. Accepted, but only one thread is used for now. |
| `Move Overhead` | spin, 0 to 5000 | 10 | Milliseconds kept back from every move's time, to cover GUI and network lag. |
| `Clear Hash` | button | | Clears the transposition table, once there is one. |
| `UCI_Chess960` | check | false | Chess960 castling notation, described below. |

It supports **Chess960**: FENs may use **X-FEN** or **Shredder-FEN** castling rights, and
with the `UCI_Chess960` option on, castling moves are read and written as the king taking
its own rook (`e1h1`) rather than as the king's move (`e1g1`). Three non-standard commands
help with debugging:

- `d` prints the current board, its FEN (Shredder-FEN with `UCI_Chess960` on) and its
  Zobrist key.
- `eval` prints the static evaluation of the current position in centipawns, from White's
  point of view.
- `go perft <depth>` prints the **Perft** count below each legal move, then the total.

Run with `bench` as its argument, it runs **Bench** instead of the UCI loop: it searches
a fixed set of 50 positions and prints the total node count, the **Bench signature**, as
its final line, `<nodes> nodes <nps> nps`. `bench <depth>` searches to a depth other than
the default of 5.

```sh
./target/release/mudskipper bench
```

## Building

Mudskipper needs Rust 1.99. The version is pinned in `rust-toolchain.toml`, so `rustup`
installs and selects it automatically. There are no runtime dependencies.

```sh
cargo build --release
./target/release/mudskipper
```

Or use the Makefile, which names the binary with the `EXE` variable:

```sh
make EXE=mudskipper
./mudskipper
```

The default build is portable: it uses only the baseline instruction set for your
target, so the binary runs on any machine of the same architecture.

## Building for SPRT

Both engines in an **SPRT** must be built the same way. Build each one from its own
checkout with `target-cpu=native`, giving each binary a distinct name:

```sh
git switch main          && make EXE=mudskipper-main CPU=native
git switch my-change     && make EXE=mudskipper-dev  CPU=native
```

`CPU=native` passes `-C target-cpu=native` to rustc. A native binary may crash with an
illegal instruction on a different CPU, so never distribute one.

The equivalent without `make` is:

```sh
cargo rustc --release --bin mudskipper -- -C target-cpu=native --emit link=mudskipper-dev
```

## Development

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same three checks on Linux and Windows. It also checks that `bench` reports
the node count given by the `Bench:` line of the head commit message.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the branch, SPRT and commit message workflow.

The deep **Perft** suite is too slow for CI. Run it by hand in release mode:

```sh
cargo test --release --test perft -- --ignored
```

Criterion benchmarks cover move generation, making moves and perft:

```sh
cargo bench
```

## License

Mudskipper is free software, licensed under the GNU General Public License v3.0 or
later. See [LICENSE](LICENSE).
