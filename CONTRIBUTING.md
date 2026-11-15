# Contributing

Mudskipper changes are tested by playing games, not just by running tests. Every change
either proves an Elo gain (or no loss) with an **SPRT**, or proves it doesn't change search
at all by keeping the **Bench signature**. The commit history records which, so it doubles
as the testing record. The terms in bold are defined in [GLOSSARY.md](GLOSSARY.md).

## Workflow

1. **One branch per change.** Branch from `main` and keep the change to one idea, so that
   one SPRT measures one thing.
2. **Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.**
3. **Run Bench** on a release build and note the node count:

   ```sh
   cargo run --release -- bench
   ```

   The final line is `<nodes> nodes <nps> nps`. The node count is the **Bench signature**.
   `bench <depth>` searches to a different depth for quicker or deeper checks, but the
   signature always uses the default depth.
4. **Run an SPRT locally** against `main` (see [SPRT setup](#sprt-setup)), unless the change
   is a [No functional change](#no-functional-change).
5. **Squash-merge** into `main` with the commit message described below. Only changes
   that pass their SPRT are merged.

## Commit message format

The squash-merge commit has a short summary line, then the SPRT summary, then the
`Bench:` line as the last line:

```text
Add a transposition table

Elo   | 48.31 +- 12.40 (95%)
SPRT  | 8.0+0.08s Threads=1 Hash=16
LLR   | 2.96 (-2.94, 2.94) [0.00, 3.00]
Games | N: 1846 W: 712 L: 461 D: 673

Bench: 8312407
```

- **Elo**: the measured Elo difference and its 95% error.
- **SPRT**: the time control, threads and hash.
- **LLR**: the final log-likelihood ratio, the acceptance bounds it crossed, and the
  Elo bounds `[elo0, elo1]` tested.
- **Games**: games played, with wins, losses and draws for the new version.
- **Bench**: the node count from `bench`, written exactly as `Bench: <nodes>`.

A change that needed both STC and LTC lists both SPRT summaries, STC first.

CI builds the head commit, runs `bench` on Linux and Windows, and fails if the node count
differs from the last `Bench:` line of the head commit message, or if there is no such
line. The failure message shows the expected and actual counts. On a pull request, the
`Bench:` line belongs in the branch's head commit; the same line goes in the squash
commit.

## No functional change

A **No functional change** commit leaves the **Bench signature** identical: refactors,
documentation, tests, tooling, or speedups that search exactly the same tree. It needs no
SPRT. Its message says so in place of the SPRT summary:

```text
Split move ordering into its own module

No functional change

Bench: 19654115
```

The CI check verifies the claim: if the node count changed, the commit changed the search,
and it needs an SPRT. A pure speedup that keeps the signature may still be worth a
non-regression SPRT to confirm the speed gain.

Some changes alter the signature without being meant to change playing strength, such as
editing the Bench positions or the default depth. These are rare; say why in the message
and give the new `Bench:` line.

## SPRT setup

SPRTs run locally with [fastchess](https://github.com/Disservin/fastchess). Build both
engines with `target-cpu=native` as described in the README's
[Building for SPRT](README.md#building-for-sprt) section.

### Time controls

- **STC**: 8+0.08 (8 seconds per game plus 0.08 seconds per move). Every functional change
  runs it.
- **LTC**: 40+0.4. Changes whose benefit should grow with depth, such as pruning and
  reductions, run it after passing STC.

Both use `Threads=1` and `Hash=16`.

### Bounds

Bounds are in normalized Elo, with `alpha=0.05` and `beta=0.05`:

| Kind of change | STC | LTC |
|---|---|---|
| Gainer: should gain Elo | `[0.00, 3.00]` | `[0.50, 2.50]` |
| Non-regression: simplification, or a needed change that must not lose | `[-3.00, 1.00]` | `[-3.00, 1.00]` |

### Openings

Games start from **DFRC** openings, which spread positions widely and exercise
**Chess960** castling. Use the `DFRC_openings.epd` book from the
[official-stockfish/books](https://github.com/official-stockfish/books) repository.
Every opening is played twice, once with each colour.

### Command

```sh
fastchess \
  -engine cmd=./mudskipper-dev name=dev \
  -engine cmd=./mudskipper-main name=main \
  -each proto=uci tc=8+0.08 option.Threads=1 option.Hash=16 \
  -variant fischerandom \
  -openings file=DFRC_openings.epd format=epd order=random \
  -games 2 -repeat -rounds 50000 \
  -concurrency 8 \
  -sprt elo0=0 elo1=3 alpha=0.05 beta=0.05 model=normalized \
  -recover \
  -pgnout file=sprt.pgn
```

For LTC, change `tc=8+0.08` to `tc=40+0.4` and the bounds to `elo0=0.5 elo1=2.5`. For
non-regression, use `elo0=-3 elo1=1`. Set `-concurrency` to the number of physical cores,
leaving one free. `-variant fischerandom` turns `UCI_Chess960` on in both engines.
