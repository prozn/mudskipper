# Mudskipper

Mudskipper is a UCI chess engine written in Rust. It speaks the
[Universal Chess Interface](https://backscattering.de/chess/uci/), so it runs inside any
UCI GUI (Cute Chess, Arena, Banksia, En Croissant) or match runner (fastchess,
cutechess-cli, OpenBench).

It is at an early stage: the binary completes the UCI handshake but does not play yet.

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

CI runs the same three checks on Linux and Windows.

## License

Mudskipper is free software, licensed under the GNU General Public License v3.0 or
later. See [LICENSE](LICENSE).
