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
LLR   | 2.96 (-2.94, 2.94) [0.00, 10.00]
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
| Gainer: should gain Elo | `[0.00, 10.00]` | `[0.00, 10.00]` |
| Non-regression: simplification, or a needed change that must not lose | `[-10.00, 0.00]` | `[-10.00, 0.00]` |

Every pair of bounds is 10 apart, so that a test finishes in about 7,000 games when the
true difference is at either bound. The number of games grows with the square of
1 / (elo1 − elo0): bounds 3 apart need about 11 times as many, more than a few cores can
play. The cost is that a gainer worth only a few normalized Elo usually fails.

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
  -sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 model=normalized \
  -recover \
  -pgnout file=sprt.pgn
```

For LTC, change `tc=8+0.08` to `tc=40+0.4`. For non-regression, use `elo0=-10 elo1=0`. Set `-concurrency` to the number of physical cores,
leaving one free. `-variant fischerandom` turns `UCI_Chess960` on in both engines.

## SPSA tuning

**Search parameters** are tuned in groups with **SPSA** on a self-hosted
[OpenBench](https://github.com/AndyGrant/OpenBench) instance, then the tuned set goes
through an SPRT as one change. SPSA finds values but proves nothing, so a tuned set is
never merged without its SPRT. OpenBench is used only for SPSA; SPRTs stay on fastchess.

### Engine entry

Set up the server and client as described in OpenBench's wiki,
[Creating Your OpenBench Instance](https://github.com/AndyGrant/OpenBench/wiki/Creating-Your-OpenBench-Instance).
Then add the engine on the server's Manage Engines page, or load this file with
`python3 manage.py import_engines mudskipper.json`:

```json
{
    "private": false,
    "nps": <bench nps of the client>,
    "source": "https://github.com/prozn/mudskipper",
    "build": {
        "path": "openbench/tune",
        "compilers": ["cargo>=1.99"],
        "cpuflags": [],
        "systems": ["Linux"]
    }
}
```

`"path": "openbench/tune"` makes OpenBench build the **Tune build** with the makefile in
that directory, which runs the root Makefile with `FEATURES=tune`, so the network is
fetched as in every other build. Replace the `nps` placeholder with the nps the client machine's
`bench` reports, which OpenBench uses to scale time controls, and set `systems` to its
operating system. A run starts straight from `main` or a PR branch; no
tuning branch is needed.

### Workload input

The **Tune build** prints the workload input from its parameter table, one line per
**Search parameter**:

```sh
make -C openbench/tune EXE=mudskipper-tune CPU=native
./openbench/tune/mudskipper-tune spsa
```

```text
RFP_MARGIN, int, 80, 40, 160, 8, 0.002
FP_BASE_MARGIN, int, 90, 20, 250, 12, 0.002
...
```

Each line is `name, int, value, min, max, C_end, R_end`. Paste the lines for the group
being tuned into the workload's parameters. C_end is about 1/20 of each parameter's
range, so that each perturbation is worth a few Elo; R_end is 0.002 for every parameter.

### Hyperparameters

Use these for every run:

| Setting | Value |
|---|---|
| α (alpha) | 0.602 |
| γ (gamma) | 0.101 |
| A-ratio | 0.1 |
| R_end | 0.002 (printed by `spsa`) |
| Pairs per iteration | 8 |
| Reporting | Batched |
| Distribution | Single |

### The run

- **Time control**: STC, 8+0.08, with `Threads=1` and `Hash=16`, and the DFRC book as for
  SPRTs, so that a tune plays under the conditions its SPRT will. If the server doesn't
  list the book, add it on the Manage Books page. Keep `FRC` in its name: OpenBench plays
  **Chess960** only when the book's name contains `FRC`, `960` or `FISCHER`.
- **Iterations**: 3,000, which at 8 pairs is 48,000 games, about 19 hours on 14 cores.
- **Stopping early**: only when no parameter has moved after about 5,000 games. Then the
  run is useless as set up: raise the C_end of the parameters that stayed put and start
  again. A run that is merely slow is left to finish.

### When a parameter ends at a bound

A parameter that ends a run at its min or max may have a better value outside the range.
Widen its range in the parameter table, keeping C_end at about 1/20 of the new range, and
tune the group again from the tuned values. Don't narrow a range to hold a parameter back:
clipping at a bound hides which way the parameter wants to move. R_end, not the range,
limits how far it moves.

### Baking in the results

1. Round the final values and write them into the parameter table as the new defaults,
   in one PR for the group.
2. Bump the patch version in `Cargo.toml`.
3. Run Bench and put the new `Bench:` line in the commit message.
4. Run an SPRT at STC `[0.00, 10.00]` (8+0.08), then at LTC `[0.00, 10.00]` (40+0.4), and
   put both summaries in the commit message.

### When the gain is too small to pass

A tuned set is often worth only a few Elo, which usually fails `[0.00, 10.00]`. Don't
narrow the bounds. Keep the branch, fold its values into the next group's tune, and
SPRT the combined set, whose gain is large enough to measure.
