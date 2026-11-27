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
4. **Run an SPRT on OpenBench** against `main` (see [SPRT setup](#sprt-setup)), unless the
   change is a [No functional change](#no-functional-change). Push the branch first:
   OpenBench builds from GitHub.
5. **Squash-merge** into `main` with the commit message described below. Only changes
   that pass their SPRT are merged.

## Commit message format

The squash-merge commit has a short summary line, then the SPRT summary, then the
`Bench:` line as the last line. The SPRT summary is the block OpenBench shows on the test's
page, copied as it is:

```text
Add a transposition table

Elo   | 48.31 +- 12.40 (95%)
SPRT  | 8.0+0.08s Threads=1 Hash=16MB
LLR   | 2.96 (-2.94, 2.94) [0.00, 10.00]
Games | N: 1846 W: 712 L: 461 D: 673
Penta | [38, 190, 393, 235, 67]

Bench: 8312407
```

- **Elo**: the measured Elo difference and its 95% error.
- **SPRT**: the time control, threads and hash.
- **LLR**: the final log-likelihood ratio, the acceptance bounds it crossed, and the
  Elo bounds `[elo0, elo1]` tested.
- **Games**: games played, with wins, losses and draws for the new version.
- **Penta**: game pairs by the new version's score in the pair, from 0 to 2 in steps of
  ½. Commits from before SPRTs moved to OpenBench have no Penta line.
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

## OpenBench

SPRTs and **SPSA** tunes run on a self-hosted [OpenBench](https://github.com/AndyGrant/OpenBench)
instance. Set up the server and client as described in OpenBench's wiki,
[Creating Your OpenBench Instance](https://github.com/AndyGrant/OpenBench/wiki/Creating-Your-OpenBench-Instance).

### Engine entries

Mudskipper needs two engines on the server, because an SPRT tests the release build and a
tune needs the **Tune build**, which is slightly slower:

| Engine | Path | Builds | Used for |
|---|---|---|---|
| `Mudskipper` | `openbench/sprt` | the release build | SPRTs |
| `Mudskipper-Tune` | `openbench/tune` | the **Tune build** | SPSA |

Add each on the server's Manage Engines page, or load a file per engine with
`python3 manage.py import_engines Mudskipper.json Mudskipper-Tune.json` (the file name is
the engine's name). Both are the same apart from the path:

```json
{
    "private": false,
    "nps": <bench nps of the client>,
    "source": "https://github.com/prozn/mudskipper",
    "build": {
        "path": "openbench/sprt",
        "compilers": ["cargo>=1.99"],
        "cpuflags": [],
        "systems": ["Linux"]
    }
}
```

- **Path**: OpenBench runs `make -j EXE=<name>` in this directory and takes the binary
  from it. Each makefile there runs the root Makefile with `CPU=native`, so the network
  named in `network.txt` is fetched as in every other build, and the binary is tuned for
  the client that builds it, which is always the one that runs it.
- **nps**: replace the placeholder with the nps that engine's `bench` reports on the
  client, built by its makefile (`make -C openbench/sprt EXE=x`, then `./openbench/sprt/x
  bench`). OpenBench scales each time control by this nps over the nps it measures, so
  the client then plays time controls as written. Measure each engine separately.
- **Compilers**: the client checks `cargo --version` in its own directory, which reports
  the rustup default toolchain, so make that 1.99 or later. Builds use the toolchain in
  `rust-toolchain.toml` either way.
- **CPU flags**: none. A client is given an engine only if it has every flag listed.
- **Systems**: the client's `platform.system()`, exactly: `Linux`, `Windows` or `Darwin`.

### Opening book

Games start from the `UHO_Lichess_4852_v1.epd` book from the
[official-stockfish/books](https://github.com/official-stockfish/books) repository
(distributed there zipped). UHO (Unbalanced Human Openings) positions are standard-chess
openings chosen so that one side has a clear edge, which makes games more decisive than
balanced openings and each game more informative, while staying close to real play. If the
server doesn't list the book, add it on the Manage Books page. Keep `FRC`, `960` and
`FISCHER` out of its name: OpenBench plays **Chess960** when the name contains any of them.
Every opening is played twice, once with each colour.

### Adjudication

Every SPRT and tune ends games early with the same settings:

- **Win**: `movecount=3 score=600`. A game is adjudicated a win once the score has stayed
  at 600 centipawns or more in one side's favour for 3 moves.
- **Draw**: `movenumber=40 movecount=8 score=10`. After move 40, a game is adjudicated a
  draw once both scores stay within 10 centipawns of zero for 8 moves.
- **Syzygy**: none. Mudskipper doesn't probe tablebases, and games aren't adjudicated by
  them, so Syzygy adjudication and Syzygy WDL are both `DISABLED` on OpenBench.

## SPRT setup

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

### Creating the test

Create a test on OpenBench with these fields:

| Field | Value |
|---|---|
| Dev engine, Base engine | `Mudskipper` |
| Dev branch | the PR branch |
| Base branch | `main` |
| Dev and Base options | `Threads=1 Hash=16` |
| Dev and Base time control | `8.0+0.08` (STC) or `40.0+0.4` (LTC) |
| Dev and Base network | empty, so that each builds with its own `network.txt` |
| Opening book | `UHO_Lichess_4852_v1.epd` |
| Win adjudication | `movecount=3 score=600` |
| Draw adjudication | `movenumber=40 movecount=8 score=10` |
| Syzygy adjudication, Syzygy WDL | `DISABLED` |
| Test mode | SPRT |
| Bounds | `[0.00, 10.00]` or `[-10.00, 0.00]`, from the table above |
| Confidence | `[0.05, 0.05]` |

OpenBench fills in each side's Bench from the last `Bench: <nodes>` line of its head
commit message, and the client checks it against the binary's `bench` before playing, so
both branches need a correct `Bench:` line. When the test ends, copy its summary into the
commit message.

### Without OpenBench

An SPRT can also run locally with [fastchess](https://github.com/Disservin/fastchess).
Build both engines with `target-cpu=native` as described in the README's
[Building for SPRT](README.md#building-for-sprt) section, and use the same time controls,
bounds, book and adjudication:

```sh
fastchess \
  -engine cmd=./mudskipper-dev name=dev \
  -engine cmd=./mudskipper-main name=main \
  -each proto=uci tc=8+0.08 option.Threads=1 option.Hash=16 \
  -openings file=UHO_Lichess_4852_v1.epd format=epd order=random \
  -resign movecount=3 score=600 \
  -draw movenumber=40 movecount=8 score=10 \
  -games 2 -repeat -rounds 50000 \
  -concurrency 8 \
  -sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 model=normalized \
  -recover \
  -pgnout file=sprt.pgn
```

For LTC, change `tc=8+0.08` to `tc=40+0.4`. For non-regression, use `elo0=-10 elo1=0`.
Set `-concurrency` to the number of physical cores, leaving one free. Write the result in
the commit message in the same form, taking Penta from fastchess's `Ptnml(0-2)` line.

## SPSA tuning

**Search parameters** are tuned in groups with **SPSA** on OpenBench, using the
`Mudskipper-Tune` engine (see [Engine entries](#engine-entries)), then the tuned set goes
through an SPRT as one change. SPSA finds values but proves nothing, so a tuned set is
never merged without its SPRT. A run starts straight from `main` or a PR branch; no
tuning branch is needed.

### Groups

Each **Search parameter** has one home group, and a run normally tunes one group. Choose
groups by three rules:

- Tune together the parameters that interact, so that SPSA can trade one against another.
- Keep apart the ones that could spoil a run (whole-number thresholds) or that don't touch
  the search (time management).
- Keep each run to about 20 parameters.

| Group | Search parameters | Purpose and when to run |
|---|---|---|
| A: evaluation margins | `RFP_MARGIN`, `FP_BASE_MARGIN`, `FP_DEPTH_MARGIN`, `SEE_PRUNING_DEPTH_MARGIN`, `ASPIRATION_DELTA` | Margins measured in evaluation units. Tuned in #84, which also held `SEE_PRUNING_HISTORY_DIVISOR`, now in C. Re-run after each new **Network** (next: **Generation** 2, #86), and for #107's `RFP_MARGIN` re-tune. |
| B: reductions and ordering | `QUIET_LMR_BASE`, `QUIET_LMR_DIVISOR`, `NOISY_LMR_BASE`, `NOISY_LMR_DIVISOR`, `VICTIM_WEIGHT`, `PROMOTION_DIVISOR` | The shape of the reduction tables and noisy move ordering. Tuned in #97, which also held the shared history bonus parameters that #98 split into group C, and `HISTORY_DIVISOR`, now in C. Re-run after a change to how late moves are reduced or noisy moves are ordered. |
| C: history shape | `QUIET_BONUS_PER_PLY`, `QUIET_BONUS_OFFSET`, `QUIET_MAX_BONUS`, `QUIET_MALUS_PER_PLY`, `QUIET_MALUS_OFFSET`, `QUIET_MAX_MALUS`, `NOISY_BONUS_PER_PLY`, `NOISY_BONUS_OFFSET`, `NOISY_MAX_BONUS`, `NOISY_MALUS_PER_PLY`, `NOISY_MALUS_OFFSET`, `NOISY_MAX_MALUS`, `HISTORY_DIVISOR`, `SEE_PRUNING_HISTORY_DIVISOR`, `NMP_BASE_REDUCTION`, `NMP_DEPTH_REDUCTION`, `LMP_BASE_MOVES`, `LMP_DEPTH_MOVES` | How history is built and how much it counts. The next search run. The two divisors belong here because both divide history, and once bonus and malus differ the typical history values move. The null-move reduction and late move pruning count terms, rewritten in finer units by #92 and never tuned, share the run rather than wait for one of their own. |
| D: integer thresholds | `RFP_MAX_DEPTH`, `FP_MAX_DEPTH`, `LMP_MAX_DEPTH`, `NMP_MIN_DEPTH`, `LMR_MIN_DEPTH`, `IIR_MIN_DEPTH`, `ASPIRATION_MIN_DEPTH`, `LMR_FULL_DEPTH_MOVES`, once #108 adds them | Depth limits and a move count, compared against whole numbers. Alone, at C_end 0.5 (the smallest C OpenBench uses for an integer, not 1/20 of the range), so that a parameter that swings spoils only its own run. After C. |
| T: time management | the time management constants, once #99 adds them | Alone: it doesn't interact with the search groups and doesn't change the **Bench signature**. Can run whenever the test box is free. Check the result at LTC as well as STC. |
| Joint polish (optional) | every parameter in A, B and C | Once each of those groups has been tuned, one run from the tuned values to catch interactions between groups. Larger than the 20-parameter rule allows, which is why it's optional. |

A new **Search parameter** gets its home group in this table in the PR that adds it. A
re-tune that mixes groups, such as one parameter folded into another group's run, names
its parameters in its own issue.

### Workload input

The **Tune build** prints the workload input from its parameter table, one line per
**Search parameter**:

```sh
make -C openbench/tune EXE=mudskipper-tune
./openbench/tune/mudskipper-tune spsa
```

```text
RFP_MARGIN, int, 49, 40, 160, 8, 0.002
FP_BASE_MARGIN, int, 104, 20, 250, 12, 0.002
...
```

Each line is `name, int, value, min, max, C_end, R_end`. Paste the lines for the group
being tuned, exactly the parameters [Groups](#groups) lists for it, into the workload's
parameters. A missed or extra line quietly changes what the run tunes. C_end is about
1/20 of each parameter's range, so that each perturbation is worth a few Elo; R_end is
0.002 for every parameter.

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

- **Time control**: STC, 8+0.08, with `Threads=1` and `Hash=16`, the UHO
  [opening book](#opening-book) and the [adjudication](#adjudication) settings, as for
  SPRTs, so that a tune plays under the conditions its SPRT will.
- **Iterations**: 3,000, which at 8 pairs is 48,000 games, about 19 hours on 14 cores.
- **Stopping early**: only when no parameter has moved after about 5,000 games. Then the
  run is useless as set up: raise the C_end of the parameters that stayed put and start
  again. A run that is merely slow is left to finish.

### Tracking the run

OpenBench keeps only each parameter's current value: the tune page's Show SPSA Digest
button gives a table of current, start, min, max, C and R, but no history and no graphs.
To see how the parameters move, which is how you tell a converged parameter from one still
drifting, record the digest with OpenBench's `Scripts/spsa/track_spsa.py` from the moment
the tune starts. Earlier values can't be recovered.

```sh
git clone https://github.com/AndyGrant/OpenBench
python3 OpenBench/Scripts/spsa/track_spsa.py --spsa <tune id> \
  -S <server URL> -U <username> -P <password>
```

- **Tune id**: the number in the tune page's URL.
- **Credentials**: your OpenBench login, or the `OPENBENCH_SERVER`, `OPENBENCH_USERNAME`
  and `OPENBENCH_PASSWORD` environment variables instead of `-S`, `-U` and `-P`. It needs
  Python's `requests` package.
- **Output**: one snapshot every 2 minutes (`--interval` sets the minutes), appended to
  `spsa.<tune id>.log` in `OpenBench/Scripts/spsa/`. Each line is a JSON array of a
  timestamp, the tune's info and the digest as CSV text.
- **Stopping**: it never stops by itself, even when the tune finishes; stop it with
  Ctrl-C once the tune is done.

Keep the log. When the tune is done, plot it for the PR with
`openbench/tune/plot_spsa.py`, which needs only Python's standard library:

```sh
python3 openbench/tune/plot_spsa.py spsa.<tune id>.log <group>.svg
```

It writes one chart per parameter, each spanning the parameter's full range with a dashed
line at the start value, and ignores snapshots after the tune finished. It also prints how
far each parameter moved over the last quarter of the games, in units of its C_end: one
still moving by more than about half a C_end hadn't settled when the run ended. Attach the
SVG to the PR with the final values.

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
