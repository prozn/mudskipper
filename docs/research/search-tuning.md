# Tuning search constants with SPSA

Research for #58 (an SPSA tuner) and #84 (re-tuning the pruning margins for the **Network**). Written 2026-11-22 against mudskipper at `7ac9fa8` (origin/main, gen1-256, `SCALE` = 143).

Each claim links to source at a pinned commit, to a first-party wiki or doc, or to James Spall's own SPSA pages. Sections 2–6 report what the sources say. Sections 7 and 8 are **recommendations**, and they say where they rest on judgement and not on a source.

Pinned sources (see the [Sources](#9-sources) list):

| Source | Commit | What it gives |
|---|---|---|
| OpenBench | `8b53673` (2026-11-17), wiki `52cb0ee` | SPSA server and client, C_end/R_end conventions |
| fishtest | `a71b7cb` (2026-11-20), wiki `f469259` | Stockfish's SPSA defaults and tuning guidance |
| Stockfish | `3e8fde7` (2026-11-22) | `TUNE` macro |
| Stormphrax | `d1468d9` (2026-08-29) | `SP_TUNABLE_PARAM`, compile-time switch |
| Viridithas | `4609ff2` (2026-11-17) | Rust; `tuning` Cargo feature |
| Berserk | `1d2354d` (2026-11-14) | X-macro table, `SPSA=1` make flag |
| Alexandria | `1c48e41` (2026-11-08) | `TUNE_PARAM` with C_end/R_end per parameter |
| Obsidian | `720614a` (2025-09-27) | `DEFINE_PARAM_S` (value, step) |
| Weather Factory | `19b4805` (2024-12-23) | standalone fastchess SPSA script |
| J. C. Spall, JHU/APL | n/a | the algorithm and its gain coefficients |

Terms follow [GLOSSARY.md](../../GLOSSARY.md): **Reverse futility pruning**, **Futility pruning**, **Late move pruning**, **SEE pruning**, **Late move reduction**, **Aspiration window**, **Quiet history**, **Noisy history**, **Bench signature**, **No functional change**, **SPRT**. "Tune build" means a build that exposes the constants as UCI options. "Release build" means the normal build.

## 1. Summary

- **Algorithm.** SPSA moves every parameter at once. Each step plays a game pair between θ + c·Δ and θ − c·Δ, where Δ is a random ±1 vector, and moves θ by a·(W − L)/(c·Δ). The step sizes shrink as `c_k = c/(k+1)^γ` and `a_k = a/(A+k+1)^α`. Spall's gain-selection code uses α = 0.602 with γ = 0.101, A = 10% of the iterations, and c ≈ the noise standard deviation ([gainsSPSA.txt](https://www.jhuapl.edu/spsa/PDF-SPSA/gainsSPSA.txt)). OpenBench and fishtest both default to α = 0.602, γ = 0.101 and A-ratio 0.1.
- **OpenBench convention.** Each parameter is one line, `name, int|float, value, min, max, C_end, R_end`. C_end is the final perturbation and R_end the final learning rate. OpenBench's advice is R_end = 0.002 and C_end ≈ 1/20 of the parameter's sensible range. Integer parameters get C ≥ 0.5 and stochastic rounding. Every engine checked prints this line from its own parameter table, and every one uses R_end = 0.002.
- **Exposing parameters.** Stormphrax, Berserk and Alexandria compile each parameter as a constant in the release build (`constexpr`/`enum`). Only a build flag turns it into a runtime variable and a UCI option. Viridithas, the one Rust engine checked, always reads a runtime `Config` struct and gates only the UCI listing behind a `tuning` feature. Tables built from parameters, such as the LMR table and SEE values, are rebuilt by a callback when an option is set.
- **What depends on the network's scale.** Only four mudskipper constants are in evaluation units: the RFP margin (80), the two futility margins (90 + 60·depth), and the aspiration delta (20). The **SEE pruning** margin is compared with static exchange material in `SEE_VALUES` units, not with the evaluation, so it does not scale with the network, even though #84 lists it. Mudskipper has no razoring, no delta pruning in quiescence, no null-move eval margin and no eval-based null-move reduction, so there is nothing of that kind to tune.
- **The scale barely moved.** PeSTO's mean |eval| over the **Bench** positions was 1.02× the pilot network's (PR #89). gen1-256 evaluates 6–10% higher than the pilot on PR #90's spot checks. Rescaling the four eval-unit margins by the eval ratio therefore changes them by about 10% at most. They are a fine SPSA starting point as they stand, and the retune is about the new evaluation's shape and accuracy rather than its units.
- **Recommendation.** Add a `tune` Cargo feature through a `macro_rules!` table. In the release build each parameter is a `const fn` returning a literal. In the tune build it is an `AtomicI32` with a UCI spin option. The feature lands as a **No functional change** PR whose CI checks that the tune build's **Bench** matches. Then tune in small groups at STC, starting with the six eval-margin and SEE-pruning parameters (#84) and then the history/LMR group, at 40–80k games each. Bake each result into the `const`s and send it for an STC and then an LTC SPRT at [0, 10]. Run the tuner on a self-hosted OpenBench instance (mudskipper already meets its engine requirements), or on Weather Factory if a server is too much.

## 2. The algorithm and its hyperparameters

### Spall's formulation

SPSA was introduced in Spall, "Multivariate Stochastic Approximation Using a Simultaneous Perturbation Gradient Approximation", *IEEE Trans. Automatic Control* 37, pp. 332–341, 1992. Its practical gain guidelines are in Spall, "Implementation of the Simultaneous Perturbation Algorithm for Stochastic Optimization", *IEEE Trans. Aerospace and Electronic Systems* 34, pp. 817–823, 1998 ([reference list](https://www.jhuapl.edu/spsa/Pages/References-List_Ref.htm)). The 1998 PDF on Spall's site is a scanned image, and I could not extract its text. The facts below come from the MATLAB code Spall publishes alongside it.

- **Core loop** ([MatlabCode-TXT.txt](https://www.jhuapl.edu/spsa/PDF-SPSA/MatlabCode-TXT.txt)): `ak=a/(k+1+A)^alpha; ck=c/(k+1)^gamma; delta=2*round(rand(p,1))-1;` (a ±1 Bernoulli vector), then `ghat=(yplus-yminus)./(2*ck*delta); theta=theta-ak*ghat`. Bounds are imposed by clamping θ after each update: "`theta=min(theta,thetamax); theta=max(theta,thetamin);`".
- **Gain selection** ([gainsSPSA.txt](https://www.jhuapl.edu/spsa/PDF-SPSA/gainsSPSA.txt)): "alpha = 0.602 is often used as a companion to gamma = 0.101". `A = .10*` the expected number of loss evaluations per run divided by `2*gavg`, which is 10% of the iterations. `c` is "the standard deviation of the measurement noise", divided by √gavg when gavg gradients are averaged. `a` is chosen so that the first step has a "desired magnitude of change in the theta elements".
- *Unverified:* the claim that 1 and 1/6 are the asymptotically optimal α and γ, and 0.602/0.101 the practical choice, is usually attributed to the 1998 paper. I could not read it there.

### What OpenBench does

OpenBench takes C_end and R_end per parameter and derives Spall's c and a from them ([spsa_utils.py:68-72](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/spsa_utils.py#L68-L72)):

```
c_value = C_end * iterations^gamma
a_value = R_end * C_end^2 * (A_ratio * iterations + iterations)^alpha
```

At iteration k it uses `c = max(c_value / k^gamma, 0.5 if int else 0)` and `r = a_value / (A_ratio·iterations + k)^alpha / c²` ([spsa_utils.py:148-154](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/spsa_utils.py#L148-L154)). So c = C_end and r = R_end exactly at the last iteration, and both are larger earlier. With the defaults, r starts about 11^0.602 ≈ 4.2 times R_end. It then:

- draws a ±1 flip per parameter, sets `dev = value + flip·c` and `base = value − flip·c`, rounds integer parameters stochastically (`floor(x + U[0,1))`), and clips both to [min, max] ([spsa_utils.py:156-171](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/spsa_utils.py#L156-L171));
- has the client add `r · c · (W − L) · flip` per batch of game pairs ([Client/worker.py:369-384](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/Client/worker.py#L369-L384)), which is Spall's `a_k · ĝ` with the loss replaced by the score;
- adds each delta and clips to [min, max] on the server ([utils.py:606-613](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/utils.py#L606-L613));
- counts one iteration per `pairs_per` game pairs, and finishes after `2 · pairs_per · iterations` games (same lines);
- passes the values to the match runner as `option.NAME=value` UCI options ([Client/worker.py:528-535](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/Client/worker.py#L528-L535)). The runner is fastchess.

Form defaults: α 0.602, γ 0.101, A-ratio 0.1, 25,000 iterations, 8 pairs per iteration ([create_workload.html:180-192](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/Templates/OpenBench/create_workload.html#L180-L192)). Those defaults come to 400,000 games.

The [OpenBench wiki](https://github.com/AndyGrant/OpenBench/wiki/SPSA-Tuning-Workloads) adds:

- "An ideal value for `R_end` is `0.0020`. An ideal value for `C_end` might be about 1/20th the reasonable range of a variable … large enough to conceivably produce a difference in strength in the engine, but not much more."
- Alpha, gamma and A-ratio come "directly from some of the original SPSA academic papers. … It is extremely unlikely that you will want to modify these values."
- "In general, I've found that using a `Batched` reporting, with a `Single` distribution, and 8 pairs-per-point is sufficiently optimal."
- **Poisoned parameters.** Small integers such as a null-move reduction of 3 tuned over 0–6 start with C several times 0.5, "leading to playing `R=1` against `R=5`. … cause the parameter to soar towards one direction". The suggested remedies are to merge such a value with finer-grained terms, or to set C_end = 0.10 and multiply R_end.
- UCI spin options cannot carry floats; report the option as a string, or scale the value to an integer.

### What fishtest does

fishtest defaults to A-ratio 0.1, α 0.602 and γ 0.101 ([spsa_workflow.py:12-17](https://github.com/official-stockfish/fishtest/blob/a71b7cbaf674ed0aaee956eefda9abb71fc8fbf2/server/fishtest/spsa_workflow.py#L12-L17)). It sets `A = A_ratio · num_games / 2`, so one iteration is one game pair ([spsa_workflow.py:360](https://github.com/official-stockfish/fishtest/blob/a71b7cbaf674ed0aaee956eefda9abb71fc8fbf2/server/fishtest/spsa_workflow.py#L360)), and uses the same c and R schedule ([spsa_workflow.py:486-500](https://github.com/official-stockfish/fishtest/blob/a71b7cbaf674ed0aaee956eefda9abb71fc8fbf2/server/fishtest/spsa_workflow.py#L486-L500)). The [fishtest wiki](https://github.com/official-stockfish/fishtest/wiki/Creating-my-first-test#tuning-with-spsa) states the update as `parameter_value + (ck * rk) * (wins - losses)`, and says:

- `ck` "must be large enough to have a measurable Elo difference during the mini match". The [Mathematics page](https://github.com/official-stockfish/fishtest/wiki/Fishtest-Mathematics#parameter-tuning) puts it at "≈ a few Elo".
- min/max "should establish the broadest feasible range within which the parameter is meaningful. If this range is set too narrowly, it could lead to unwarranted clipping … To confine the evolution range for a parameter, simply set a smaller `rk` value."
- "tuning many values at once (like a PSQT table) generates random change; while values that are only rarely used to score a position can have a hard time moving at all. … a higher ck forcing value change and a lower one reducing random change."
- "If after a few thousand games the values are barely changing at all, the tuning run is useless and should be stopped."
- For search parameters, "A short TC is appropriate to get faster approximations … A long TC (`60+0.6`) is best to get better scaling values."

## 3. How engines expose parameters

| Engine | Release build | Tune build | Default range and C_end | Source |
|---|---|---|---|---|
| Stockfish | plain `const` (made non-const only on a tuning branch) | `TUNE(...)` registers `int`s as UCI options | range [0, 2v]; prints C = range / 20, R = 0.0020 | [tune.h:37-38, 55-78](https://github.com/official-stockfish/Stockfish/blob/3e8fde75412e7c47fc140cd82022a395bc681d92/src/tune.h#L37-L78), [tune.cpp:62-67](https://github.com/official-stockfish/Stockfish/blob/3e8fde75412e7c47fc140cd82022a395bc681d92/src/tune.cpp#L62-L67) |
| Stormphrax | `constexpr i32 Name()` returning the default | `SP_EXTERNAL_TUNE`: a registered `TunableParam` behind `Name()` | explicit (default, min, max, step) per parameter; `static_assert`s on the range and step ≥ 0.5 | [tunable.h:55-127](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L55-L127) |
| Viridithas | runtime `Config` struct, defaults from `const`s | `--features tuning` lists the fields as UCI options | explicit [value, min, max, step] | [Cargo.toml:19](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml#L19), [uci.rs:113-119](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/uci.rs#L113-L119), [parameters.rs:422-470](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search/parameters.rs#L422-L470) |
| Berserk | `enum { NAME = v }` | `make SPSA=1`: an `int` per entry, from one X-macro list | range in the list, mostly [0, 2v]; C_end = max(0.5, \|v\| / 10) | [tune.h:25-153](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h#L25-L153), [makefile:12-16](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/makefile#L12-L16), [uci.c:465-484](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/uci.c#L465-L484) |
| Alexandria | `constexpr int Name()` | `-DTUNE`; time management separately under `-DTUNE_TM` | explicit (default, min, max, C_end, R_end) | [tune.h:17-46](https://github.com/PGG106/Alexandria/blob/1c48e415cf55ab6a7579ae323ec800da28a0b044/src/tune.h#L17-L46) |
| Obsidian | always a runtime `EngineParam` | always listed | (value, step) gives range value ± 10·step; C_end = max(0.5, range / 20) | [tuning.h:15-37](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/tuning.h#L15-L37), [tuning.cpp:32-44](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/tuning.cpp#L32-L44) |

Common features:

- **One table, many outputs.** Each engine prints its OpenBench input from the same table that defines the parameters: Stormphrax `printob` ([uci.cpp:1132-1145](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/uci/uci.cpp#L1132-L1145)), Viridithas `emit_csv_for_spsa` ([parameters.rs:565-575](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search/parameters.rs#L565-L575)), Berserk's `spsa` command ([uci.c:465-484](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/uci.c#L465-L484)). Stormphrax and Viridithas also print Weather Factory JSON.
- **Derived tables are rebuilt on set.** Stormphrax's `quietLmrBase`/`quietLmrDivisor` carry an `updateQuietLmrTable` callback ([tunable.h:270-274](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L270-L274)). Berserk calls `InitPruningAndReductionTables()` and `InitSeeValues()` after any tunable is set ([uci.c:486-500](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/uci.c#L486-L500)).
- **Floats become scaled integers.** Stormphrax's `SP_TUNABLE_PARAM_F64` stores value × Q as an integer ([tunable.h:107-113](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L107-L113)), and its LMR bases and divisors are tuned ×100 (`quietLmrBase` 78 = 0.78). The fishtest wiki recommends multiplying small values "up" (×10) so that C can exceed 1.
- **A tuning branch flips the default.** Berserk's makefile reads "SPSA=1 turns the tunables in tune.h into UCI options (set the default to 1 on a tuning branch)" ([makefile:12](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/makefile#L12)), because OpenBench builds with a plain `make EXE=…`.
- **The framework lands as a no-functional-change commit.** Berserk's "table-driven UCI, SPSA tunables" commit is marked "Non-functional" ([ad9cc18](https://github.com/jhonnold/berserk/commit/ad9cc18191)).

### Typical ranges and steps for the terms mudskipper has

Values are each engine's own, in each engine's own units. Their formulas carry extra terms (improving, history, complexity), so the raw numbers are not directly comparable with mudskipper's.

| Term | Stormphrax | Viridithas | Berserk | Alexandria | Obsidian |
|---|---|---|---|---|---|
| RFP margin per ply | 85 [25, 150] step 5 | 65 [16, 256] step 10 | 83 [0, 166] | 73 [40, 200] C 10 | 87, step 8 |
| Futility base | 274 [120, 400] step 20 | 86 [8, 256] step 10 | 66 [0, 132] | 215 [40, 300] C 10 | 159, step 26 |
| Futility per ply | 68 [30, 120] step 4 | 70 [8, 256] step 10 | 44 [0, 88] | 112 [40, 200] C 10 | 153, step 13 |
| Noisy SEE pruning | −111·d [−120, −40] step 20 | −28·d² [−256, −1] step 3 | 108.3 (float) | −27 [−100, −1] C 3 | −96, step 10 |
| SEE history divisor | 65 [12, 256] step 10 | — | — | — | — |
| Aspiration delta | 7 [1, 50] step 2 | 12 [1, 64] step 1 | 8 [1, 16] | 8 [2, 15] C 2 | — |
| LMR base / divisor (×100) | 78 [50, 120] / 236 [100, 300], step 10 / 20 | — | 0.17 / 2.14 (float) | — | 99 / 314, step 10 / 29 |
| History bonus per ply | 277 [128, 512] step 32 | — | 191 [0, 382] | — | 175, step 15 |
| History bonus max | 2723 [1024, 4096] step 256 | — | 1708 [0, 3416] | — | 1409, step 100 |
| LMR history divisor | — | [1, 65536] step 512 | 6144 [0, 12288] | — | — |

Sources: [Stormphrax tunable.h:182-315](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L182-L315) and its use in [search.cpp:840-849, 1044-1056](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1044-L1056); [Viridithas search.rs:57-71](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L57-L71), [parameters.rs:422-470](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search/parameters.rs#L422-L470), [search.rs:1189-1190, 1459-1460](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1185-L1192); [Berserk tune.h:25-127](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h#L25-L127); [Alexandria tune.h:137-215](https://github.com/PGG106/Alexandria/blob/1c48e415cf55ab6a7579ae323ec800da28a0b044/src/tune.h#L137-L215); [Obsidian search.cpp:19-74](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/search.cpp#L19-L74).

Across these tables, C_end sits at about 1/20 of the range: Stormphrax's 85 [25, 150] step 5 is 1/25, and Obsidian's is exactly 1/20 by construction. R_end is 0.002 everywhere.

## 4. Running SPSA without a fleet

- **OpenBench, self-hosted.** The server is a Django app (`pip3 install -r requirements.txt`, `manage.py migrate`, `runserver`), and the engine is added to `Config/config.json` ([wiki: Creating Your OpenBench Instance](https://github.com/AndyGrant/OpenBench/wiki/Creating-Your-OpenBench-Instance)). Engine requirements ([wiki: Requirements](https://github.com/AndyGrant/OpenBench/wiki/Requirements-For-Public-Engines)): `Hash` and `Threads` options, a makefile taking `EXE=`, `./binary bench` printing `<nodes> nodes <nps> nps`, determinism, and `EVALFILE=` for networks. Mudskipper's [Makefile](../../Makefile) already takes `EXE=`, `CPU=` and `FEATURES=`, and **Bench** ends with `<nodes> nodes <nps> nps` ([CONTRIBUTING.md](../../CONTRIBUTING.md#L19)). The wiki also found no difference in tune quality between "a handful of workers" and the whole fleet ([SPSA wiki](https://github.com/AndyGrant/OpenBench/wiki/SPSA-Tuning-Workloads)).
- **Weather Factory.** A Python script that drives fastchess or cutechess directly ([README](https://github.com/jnlt3/weather-factory/blob/19b4805c9a2372955c29666118070269f34aa2eb/README.md)). Parameters are JSON with `value, min_value, max_value, step`, and "the step parameter should be large enough to create a 2-3 elo difference". Its `spsa.json` uses `a = c = 1`, α 0.601, γ 0.102, and the README says "Ideally `A = max iterations / 10`" ([spsa.json](https://github.com/jnlt3/weather-factory/blob/19b4805c9a2372955c29666118070269f34aa2eb/spsa.json)). It scales each perturbation by the parameter's `step` and rounds to the nearest integer, not stochastically ([spsa.py:19-27, 67-94](https://github.com/jnlt3/weather-factory/blob/19b4805c9a2372955c29666118070269f34aa2eb/spsa.py#L67-L94)). Its last commit is 2024-12-23.
- **fastchess** alone has no SPSA mode. OpenBench's client uses it as the match runner ([worker.py:437](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/Client/worker.py#L437)).

### Game budgets and time controls seen in practice

| Tune | Games or iterations | TC | Result | Source |
|---|---|---|---|---|
| Stockfish VLTC search tune | 44k games | 120+1.2 | passed VLTC [0, 2] | [36db936](https://github.com/official-stockfish/Stockfish/commit/36db936e769a2e7a95fc4032eec3b79251bbaef5) |
| Stormphrax | 3103 and 6384 iterations | "LTC" | merged | [8b5dc66](https://github.com/Ciekce/Stormphrax/commit/8b5dc666a3), [f744d7c](https://github.com/Ciekce/Stormphrax/commit/f744d7c954f77e7d652afe6480bdf796438239e6) |
| Berserk, "a large majority of the parameters" | — | tuned at LTC | **−14.1 at 10+0.1, +8.7 at 60+0.6** | [e4a2148](https://github.com/jhonnold/berserk/commit/e4a2148e0b7d9e88984ecb786128809fd8bb62d6) |
| Berserk | 7016 iterations | — | +5.1 at 60+0.6 | [787b2e1](https://github.com/jhonnold/berserk/commit/787b2e1bfa5a363860f1a4ed52c67910188ec681) |
| Alexandria, partial | 25k iterations | — | +1.7 at 8+0.08 | [a4ff995](https://github.com/PGG106/Alexandria/commit/a4ff99549d) |
| Viridithas | — | 20+0.2, 4 threads | merged | [59c995a](https://github.com/cosmobobak/viridithas/commit/59c995aa1e585cca51c02ee27db3d13a7398b51c) |

In OpenBench, games = 2 × pairs-per × iterations, so 6384 iterations at the default 8 pairs would be about 102k games. The commits do not state pairs-per. The Berserk row is the clearest evidence that a tune can win at the TC it was tuned at and lose at another.

### Retuning after moving to NNUE

- Stockfish merged NNUE on 2020-08-06 ([84f3e86](https://github.com/official-stockfish/Stockfish/commit/84f3e867903f62480c33243dd0ecbffd342796fc)). Four days later an "Assorted search parameter tune" moved the razor margin 527 → 510, the futility margin 227·d → 223·d and the aspiration delta 19 → 17, along with LMR and time-management constants ([220ef1d](https://github.com/official-stockfish/Stockfish/commit/220ef1d27d9cd006a30b07ab726999c8181d10f0)). Those are changes of 3–10%, passed at STC and LTC. The commit does not say it was motivated by NNUE. It is only the first broad tune after the switch.
- I found no engine source or first-party doc that prescribes rescaling margins by an eval-scale ratio. Stockfish adjusted the network's output scale instead, in several "NNUE scaling" commits (for example [4bb11e8](https://github.com/official-stockfish/Stockfish/commit/4bb11e823f), "Tune NNUE scaling params"), which leaves the margins' units unchanged. Rescaling by the ratio is a reasonable first guess, but it is my inference, not a sourced practice.

## 5. What mudskipper has today

Line numbers are at `7ac9fa8`. "Units" says what each constant is compared with. **Eval** means the network's output, which depends on `SCALE` and on the network. **SEE** means `SEE_VALUES` material ([see.rs:15](../../src/chess/board/see.rs#L15): 100/300/300/500/900). **Hist** means history values (±16384).

### Pruning, reductions and windows ([worker.rs](../../src/search/worker.rs))

| Constant | Line | Value | Gates | Units |
|---|---|---|---|---|
| `RFP_MARGIN` | [78](../../src/search/worker.rs#L78) | 80 | **Reverse futility pruning**: `eval − 80·depth ≥ beta` ([831-836](../../src/search/worker.rs#L831-L836)) | **eval** |
| `RFP_MAX_DEPTH` | [73](../../src/search/worker.rs#L73) | 7 | RFP depth limit | plies |
| `FP_BASE_MARGIN` | [87](../../src/search/worker.rs#L87) | 90 | **Futility pruning**: `eval + 90 + 60·depth ≤ alpha` ([882-883](../../src/search/worker.rs#L882-L883)) | **eval** |
| `FP_DEPTH_MARGIN` | [91](../../src/search/worker.rs#L91) | 60 | same | **eval** |
| `FP_MAX_DEPTH` | [83](../../src/search/worker.rs#L83) | 6 | futility and SEE pruning depth limit | plies |
| `LMP_MAX_DEPTH` | [96](../../src/search/worker.rs#L96) | 8 | **Late move pruning** depth limit | plies |
| `LMP_BASE_MOVES` | [101](../../src/search/worker.rs#L101) | 3 | LMP count `3 + depth²/2` ([884](../../src/search/worker.rs#L884)) | moves |
| `SEE_PRUNING_DEPTH_MARGIN` | [110](../../src/search/worker.rs#L110) | 90 | **SEE pruning**: `see_ge(mv, min(−90·d − hist/64, 0))` ([918-920](../../src/search/worker.rs#L918-L920)) | **SEE** |
| `SEE_PRUNING_HISTORY_DIVISOR` | [115](../../src/search/worker.rs#L115) | 64 | same | hist → SEE |
| `NMP_MIN_DEPTH` | [45](../../src/search/worker.rs#L45) | 3 | null-move depth limit | plies |
| `NMP_BASE_REDUCTION`, `NMP_DEPTH_DIVISOR` | [50](../../src/search/worker.rs#L50), [54](../../src/search/worker.rs#L54) | 3, 3 | reduction `3 + depth/3` ([571](../../src/search/worker.rs#L571)) | plies |
| (null-move eval condition) | [563](../../src/search/worker.rs#L563) | `eval ≥ beta` | no margin | eval, no constant |
| `ASPIRATION_DELTA` | [40](../../src/search/worker.rs#L40) | 20 | first half-width, then `delta += delta/2` ([294-299](../../src/search/worker.rs#L294-L299)) | **eval** |
| `ASPIRATION_MIN_DEPTH` | [34](../../src/search/worker.rs#L34) | 4 | first windowed iteration | plies |
| `IIR_MIN_DEPTH` | [28](../../src/search/worker.rs#L28) | 4 | internal iterative reduction | plies |
| `LMR_MIN_DEPTH`, `LMR_FULL_DEPTH_MOVES` | [59](../../src/search/worker.rs#L59), [67](../../src/search/worker.rs#L67) | 3, 3 | **Late move reduction** eligibility | plies, moves |

### History, ordering and LMR tables

| Constant | Where | Value | Role | Units |
|---|---|---|---|---|
| `BONUS_PER_PLY`, `BONUS_OFFSET`, `MAX_BONUS` | [history.rs:16-35](../../src/search/history.rs#L16-L35) | 300, 250, 1500 | `min(300·d − 250, 1500)`, the bonus and the malus for both tables | hist |
| `MAX_HISTORY` | [history.rs:11](../../src/search/history.rs#L11) | 16384 | gravity bound; stored as `i16` | hist |
| `HISTORY_DIVISOR` | [reduction.rs:17](../../src/search/reduction.rs#L17) | 8192 | one ply less reduction per 8192 positive history | hist |
| `QUIET` formula | [reduction.rs:48-53](../../src/search/reduction.rs#L48-L53) | 0.75 + ln·ln / 2.25 | quiet LMR table | plies |
| `NOISY` formula | [reduction.rs:60-65](../../src/search/reduction.rs#L60-L65) | 0.14 + ln·ln / 3.65 | noisy LMR table | plies |
| `VICTIM_WEIGHT`, `PROMOTION_DIVISOR` | [ordering.rs:19, 24](../../src/search/ordering.rs#L19-L24) | 16, 10 | noisy ordering score | SEE-like values |
| `ORDER_VALUES` | [ordering.rs:12](../../src/search/ordering.rs#L12) | 100/300/300/500/900 | victim values for ordering | material |
| good/bad split | [ordering.rs:201](../../src/search/ordering.rs#L201) | `see_ge(mv, 0)` | which noisy moves come before quiets | SEE |

The LMR tables are `static`s built by a `const fn` at compile time, in integer arithmetic so that the **Bench signature** is the same on every platform ([reduction.rs:1-3, 85-92](../../src/search/reduction.rs#L85-L92)). `build_table` is an ordinary `const fn`, so it can also run at run time.

### Time management ([time.rs:7-18](../../src/search/time.rs#L7-L18))

`DEFAULT_MOVES_TO_GO` 30, `INCREMENT_QUARTERS` 3, `OVERRUN_FACTOR` 3, `CEILING_QUARTERS` 3. These do not depend on the evaluation. Alexandria tunes time management in a separate build (`TUNE_TM`, [tune.h:32-46](https://github.com/PGG106/Alexandria/blob/1c48e415cf55ab6a7579ae323ec800da28a0b044/src/tune.h#L32-L46)).

### Not present

Razoring, delta or futility pruning in quiescence (quiescence stands pat and skips only `see_ge(mv, 0)` losers, [worker.rs:632-650](../../src/search/worker.rs#L632-L650)), an "improving" flag, ProbCut, singular extensions, an eval-dependent null-move reduction, and a null-move eval margin. Each would be a new feature with its own SPRT before it had anything to tune.

### Tuning infrastructure

There is none. The UCI options are only `UCI_Chess960`, `Hash`, `Threads`, `Move Overhead` and `Clear Hash` ([options.rs:34-62](../../src/uci/options.rs#L34-L62)). **Bench** exists: 50 positions at depth 8, 16 MB hash ([bench.rs:11-19](../../src/tools/bench.rs#L11-L19)), checked by CI against the commit's `Bench:` line ([CONTRIBUTING.md](../../CONTRIBUTING.md#L52)). The crate has no runtime dependencies, and `unsafe_code = "deny"` ([Cargo.toml](../../Cargo.toml)), per [ADR-0005](../adr/0005-dependencies-and-unsafe.md).

### The network's scale

- `SCALE` = 143 is the trainer's eval scale, the K that best fits `sigmoid(score / K)` to the pilot data's results ([network.rs:25](../../src/eval/network.rs#L25); PR #89).
- The mean |eval| of PeSTO over the network's, across the 50 **Bench** positions, was 1.02 for pilot-128 (wdl40) at 143, inside the target band of 0.75–1.33 (PR #89).
- gen1-256 kept 143 and evaluates a little higher than the pilot: 1149 against 1048 a queen up, 395 against 372 a knight up (PR #90). Nobody has measured its ratio against PeSTO over all 50 positions.

So the margins tuned against PeSTO are in about the right units already. What changed is how accurate and how smooth the evaluation is. That is what the retune has to find, and it cannot be computed from a ratio.

## 6. Pitfalls the sources name

- **C too small: nothing moves.** "If after a few thousand games the values are barely changing at all, the tuning run is useless" ([fishtest wiki](https://github.com/official-stockfish/fishtest/wiki/Creating-my-first-test#tuning-with-spsa)).
- **C too large on small integers ("poisoned parameters").** A depth limit or a reduction of 3 swings to an extreme and can spoil the whole run ([OpenBench wiki](https://github.com/AndyGrant/OpenBench/wiki/SPSA-Tuning-Workloads)). OpenBench forces C ≥ 0.5 for integers ([spsa_utils.py:153](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/spsa_utils.py#L153)).
- **Too many parameters at once makes random change** ([fishtest wiki](https://github.com/official-stockfish/fishtest/wiki/Creating-my-first-test#tuning-with-spsa)). Fleets tune dozens of parameters at a time (Berserk e4a2148). A single PC sees far fewer games per parameter.
- **Ranges too narrow.** Clipping hides the gradient. Confine movement with R, not with the range ([fishtest wiki](https://github.com/official-stockfish/fishtest/wiki/Creating-my-first-test#tuning-with-spsa)). A value that ends at a bound means the range was wrong, and the run should be repeated with a wider one.
- **Zero defaults.** With Stockfish's default range [0, 2v], a parameter whose value is 0 never moves ([fishtest wiki](https://github.com/official-stockfish/fishtest/wiki/Creating-my-first-test#tuning-with-spsa)). Mudskipper's noisy LMR base (0.14) would need an explicit range that crosses zero, as Stormphrax's `noisyLmrBase` −10 [−50, 75] does.
- **TC dependence.** Berserk e4a2148 lost 14 Elo at STC and gained 8.7 at LTC with the same tune. fishtest recommends 60+0.6 for "scaling values".
- **The Bench changes.** Every tune that moves a value changes the **Bench signature**: each tune commit in the table in section 4 carries a new `Bench:` line. Only the framework itself can be a **No functional change**.
- **Determinism.** OpenBench requires a deterministic `bench` on every client ([wiki: Requirements](https://github.com/AndyGrant/OpenBench/wiki/Requirements-For-Public-Engines)). Mudskipper's integer LMR tables already exist for this reason.

## 7. Recommendation for mudskipper

This section is recommendation. Where it goes beyond the sources above, it says so.

### 7.1 The tunable mechanism (one **No functional change** PR)

Requirements: zero cost in the release build, no new dependency and no `unsafe` (ADR-0005), one table that is the single source of truth, and an identical **Bench** in both builds.

Shape, after Stormphrax and Alexandria's compile-time switch, written as a Rust `macro_rules!` table:

```rust
// src/search/params.rs
//! Search constants SPSA can tune. In a normal build each is a `const fn` returning
//! its value; with the `tune` feature each is an atomic set through a UCI option.

tunables! {
    /// Reverse futility pruning margin per ply, in centipawns of evaluation.
    rfp_margin: 80, 40..=160, c_end 8;
    /// Futility pruning margin at any depth, in centipawns of evaluation.
    fp_base_margin: 90, 20..=250, c_end 12;
    // ...
}
```

The macro expands to:

- **Release build** (`#[cfg(not(feature = "tune"))]`): `pub(crate) const fn rfp_margin() -> i32 { 80 }`. A call to it is a literal after inlining, exactly as the `const` is today.
- **Tune build** (`#[cfg(feature = "tune")]`): `static RFP_MARGIN: AtomicI32 = AtomicI32::new(80);` and `pub(crate) fn rfp_margin() -> i32 { RFP_MARGIN.load(Ordering::Relaxed) }`. UCI only sets options between searches, so `Relaxed` is enough, and `AtomicI32` needs no `unsafe`. The macro also emits `const PARAMS: &[Param]`, where each entry holds the name, default, min, max, C_end and a `&'static AtomicI32`.
- **Compile-time checks** in both builds: `const _: () = assert!(MIN <= DEFAULT && DEFAULT <= MAX && MIN < MAX);`, as Stormphrax's `static_assert`s do.

The rest of the tune build:

- `uci` lists each entry as `option name RFP_MARGIN type spin default 80 min 40 max 160`, and `setoption` clamps and stores the value. `Options` already has a `Spin` type ([options.rs:19-58](../../src/uci/options.rs#L19-L58)).
- A `spsa` command, either a CLI subcommand next to `bench` or a UCI extension, prints `NAME, int, 80.0, 40.0, 160.0, 8, 0.002` for OpenBench. A `spsa json` variant can print Weather Factory's format.
- **Fractions become integers.** The LMR formulas already use exact fractions. Expose base and divisor ×100 (`quiet_lmr_base` 75, `quiet_lmr_divisor` 225, `noisy_lmr_base` 14, `noisy_lmr_divisor` 365), as Stormphrax does, and keep the release tables as compile-time `static`s built from the same defaults. In the tune build, `setoption` rebuilds a heap table with the same `build_table` `const fn`, and each `Worker` takes a copy at the start of a search, so the hot path never takes a lock.
- `Cargo.toml` gets `tune = []` under `[features]`, beside `datagen`, with a comment. A tuning branch sets `FEATURES ?= tune` in the Makefile, as Berserk's does, so that OpenBench's plain `make EXE=…` builds the tune build.
- **Acceptance:** release-build **Bench** unchanged. CI also runs `bench` on a `--features tune` build and requires the same node count, which proves the defaults are wired identically. No NPS loss in the release build.

Only parameters in the current tuning group need to be in the table at first. Moving a `const` into the table is a one-line, no-functional-change edit.

### 7.2 Hyperparameters

- α 0.602, γ 0.101, A-ratio 0.1, R_end 0.002, as Spall, OpenBench, fishtest and every engine checked use.
- OpenBench: Batched reporting, Single distribution, 8 pairs per iteration (the wiki's advice).
- C_end ≈ range / 20, checked against "a few Elo". If a parameter has not moved after the first ~5k games, raise its C_end. If it random-walks between bounds, lower it.

### 7.3 What to tune, in what order

**Group A: margins for the network (#84).** Six parameters, all of which the new evaluation's accuracy affects.

| Parameter | Start | Range | C_end | Note |
|---|---|---|---|---|
| `RFP_MARGIN` | 80 | 40–160 | 8 | eval units |
| `FP_BASE_MARGIN` | 90 | 20–250 | 12 | eval units |
| `FP_DEPTH_MARGIN` | 60 | 20–160 | 7 | eval units |
| `SEE_PRUNING_DEPTH_MARGIN` | 90 | 30–180 | 8 | SEE units, not eval |
| `SEE_PRUNING_HISTORY_DIVISOR` | 64 | 16–256 | 10 | |
| `ASPIRATION_DELTA` | 20 | 5–50 | 2 | eval units |

The ranges are about 0.5×–2× of today's values and cover every reference engine's value in section 3. The C_end values follow the 1/20 rule. Start from today's values: the eval ratio is within about 10% of 1 (section 5), so a rescale would change nothing that SPSA does not find anyway. If a full 50-position measurement of gen1 against PeSTO's numbers from PR #89 lands outside 0.9–1.1, multiply the four eval-unit starts by it first.

**Group B: history and reductions.** These interact, so they belong in one run.

| Parameter | Start | Range | C_end |
|---|---|---|---|
| `BONUS_PER_PLY` | 300 | 100–600 | 25 |
| `BONUS_OFFSET` | 250 | 0–600 | 30 |
| `MAX_BONUS` | 1500 | 500–3000 | 120 |
| `HISTORY_DIVISOR` (LMR) | 8192 | 2048–16384 | 600 |
| `quiet_lmr_base` ×100 | 75 | 25–150 | 7 |
| `quiet_lmr_divisor` ×100 | 225 | 150–350 | 12 |
| `noisy_lmr_base` ×100 | 14 | −50–100 | 8 |
| `noisy_lmr_divisor` ×100 | 365 | 200–550 | 18 |
| `VICTIM_WEIGHT` | 16 | 4–48 | 2 |

Keep `MAX_HISTORY` fixed. It bounds the `i16` storage, and every history divisor is measured against it. Splitting bonus from malus (#58's list) is a code change, so it needs its own SPRT before it can be tuned.

**Group C: small integers. Leave them out of SPSA for now.** These are the depth limits (`RFP_MAX_DEPTH`, `FP_MAX_DEPTH`, `LMP_MAX_DEPTH`, `NMP_MIN_DEPTH`, `LMR_MIN_DEPTH`, `IIR_MIN_DEPTH`, `ASPIRATION_MIN_DEPTH`) and the counts (`LMP_BASE_MOVES`, `LMR_FULL_DEPTH_MOVES`, `NMP_BASE_REDUCTION`, `NMP_DEPTH_DIVISOR`). They are OpenBench's "poisoned parameters". Before tuning one, rewrite it in finer units. For example, the null-move reduction could become `(base256 + depth · per_ply256) / 256`. Check that the defaults keep the **Bench**, or else SPRT the rewrite on its own.

**Later:** time management in its own run (Alexandria keeps it separate), after the search groups.

### 7.4 Budget and time control

*Assumption to check:* an 8+0.08 game takes about 20 s of wall time, so 20 concurrent games (#58) give about 3,600 games an hour. The true rate is in the user's own SPRT logs.

- Tune at **STC (8+0.08)**: 2,500–5,000 iterations × 8 pairs, which is 40k–80k games, about 11–22 hours per group. That is far below OpenBench's 400k default and the fleets' runs, but Stockfish's VLTC tune converged usefully in 44k games, and these groups are small. *Judgement:* about 6–10 parameters per run on one PC, after fishtest's "many values … generates random change".
- An LTC (40+0.4) tune costs five times as much. Run one only if a group's STC tune passes at STC and fails at LTC, the reverse of Berserk e4a2148.
- Watch the parameter graphs (OpenBench `Scripts/spsa/`, or Weather Factory's `graph.py`). Stop early only if nothing moves.

### 7.5 Tool

Self-host OpenBench on the tuning PC (server and client together). Mudskipper already meets its engine requirements (section 4), and it gives stochastic integer rounding, the C_end/R_end convention every engine's tables use, and SPRT workloads on the same box. Weather Factory is the lighter alternative: no server, fastchess directly. But it rounds to the nearest integer, so give it C_end ≥ 1 and not 0.5 for integer parameters, and its schedule differs from OpenBench's.

## 8. Validation plan

1. **Framework PR** (7.1): **No functional change**, Bench identical in both builds, no SPRT. A non-regression SPRT is needed only if release-build NPS drops.
2. **Measure gen1's scale** over the 50 **Bench** positions against the PeSTO numbers in PR #89, and record it in the #84 PR. If it is outside 0.9–1.1, rescale the eval-unit starting values.
3. **Tune group A** at STC. Bake the result into the `const`s, round integers, note any parameter that ended at a bound, and widen and re-tune it if the gain warrants.
4. **PR with the tuned set**, using #84's "one tuned set" option: the new `Bench:` line and the placeholder `SPRT: pending (STC gainer [0.00, 10.00])`. The user runs STC [0, 10] at 8+0.08 and then, since these are pruning changes, LTC [0, 10] at 40+0.4 ([CONTRIBUTING.md](../../CONTRIBUTING.md#L88-L91)).
5. **If the gain is too small for [0, 10].** CONTRIBUTING notes that a gainer worth only a few normalized Elo usually fails these bounds. Fold the group A and group B tunes into one set and SPRT them together, rather than narrowing the bounds.
6. **Repeat for group B**, and re-tune group A after **Generation** 2 if its scale check moves.

## 9. Sources

**SPSA (J. C. Spall, JHU/APL)**
- [SPSA home](https://www.jhuapl.edu/spsa/) · [Reference list](https://www.jhuapl.edu/spsa/Pages/References-List_Ref.htm) (Spall 1992 TAC 37:332–341; Spall 1998 TAES 34:817–823)
- [Basic SPSA MATLAB code](https://www.jhuapl.edu/spsa/PDF-SPSA/MatlabCode-TXT.txt) · [Gain selection code](https://www.jhuapl.edu/spsa/PDF-SPSA/gainsSPSA.txt)
- [Spall 1998, Implementation…](https://www.jhuapl.edu/spsa/PDF-SPSA/Spall_Implementation_of_the_Simultaneous.PDF): scanned, text not extracted

**OpenBench**: [`AndyGrant/OpenBench@8b53673`](https://github.com/AndyGrant/OpenBench/tree/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c)
- [OpenBench/spsa_utils.py](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/spsa_utils.py)
- [OpenBench/utils.py:606-613](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/OpenBench/utils.py#L606-L613)
- [Client/worker.py](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/Client/worker.py#L369-L384)
- [Templates/OpenBench/create_workload.html:180-192](https://github.com/AndyGrant/OpenBench/blob/8b5367399533eaa3f697d6b39bf8b5cd6a9c3a5c/Templates/OpenBench/create_workload.html#L180-L192)
- Wiki (`52cb0ee`): [SPSA Tuning Workloads](https://github.com/AndyGrant/OpenBench/wiki/SPSA-Tuning-Workloads), [Requirements For Public Engines](https://github.com/AndyGrant/OpenBench/wiki/Requirements-For-Public-Engines), [Creating Your OpenBench Instance](https://github.com/AndyGrant/OpenBench/wiki/Creating-Your-OpenBench-Instance)

**fishtest / Stockfish**
- [`fishtest@a71b7cb` server/fishtest/spsa_workflow.py](https://github.com/official-stockfish/fishtest/blob/a71b7cbaf674ed0aaee956eefda9abb71fc8fbf2/server/fishtest/spsa_workflow.py)
- Wiki (`f469259`): [Creating my first test § Tuning with SPSA](https://github.com/official-stockfish/fishtest/wiki/Creating-my-first-test#tuning-with-spsa), [Fishtest Mathematics § Parameter Tuning](https://github.com/official-stockfish/fishtest/wiki/Fishtest-Mathematics#parameter-tuning)
- [`Stockfish@3e8fde7` src/tune.h](https://github.com/official-stockfish/Stockfish/blob/3e8fde75412e7c47fc140cd82022a395bc681d92/src/tune.h), [src/tune.cpp](https://github.com/official-stockfish/Stockfish/blob/3e8fde75412e7c47fc140cd82022a395bc681d92/src/tune.cpp#L49-L67)
- Commits: [84f3e86](https://github.com/official-stockfish/Stockfish/commit/84f3e867903f62480c33243dd0ecbffd342796fc) (NNUE added), [220ef1d](https://github.com/official-stockfish/Stockfish/commit/220ef1d27d9cd006a30b07ab726999c8181d10f0), [36db936](https://github.com/official-stockfish/Stockfish/commit/36db936e769a2e7a95fc4032eec3b79251bbaef5), [4bb11e8](https://github.com/official-stockfish/Stockfish/commit/4bb11e823f)

**Stormphrax**: [`Ciekce/Stormphrax@d1468d9`](https://github.com/Ciekce/Stormphrax/tree/d1468d99e3d3733100d20d682ac627998cce94b2)
- [src/tunable.h](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h), [src/uci/uci.cpp:1090-1145](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/uci/uci.cpp#L1090-L1145), [src/search.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L832-L1056)
- Commits: [8b5dc66](https://github.com/Ciekce/Stormphrax/commit/8b5dc666a3), [f744d7c](https://github.com/Ciekce/Stormphrax/commit/f744d7c954f77e7d652afe6480bdf796438239e6)

**Viridithas**: [`cosmobobak/viridithas@4609ff2`](https://github.com/cosmobobak/viridithas/tree/4609ff2a898160f1576fe83c0529a949148541c0)
- [Cargo.toml](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml#L16-L22), [src/search/parameters.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search/parameters.rs), [src/uci.rs:113-119](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/uci.rs#L113-L119), [src/search.rs:57-71](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L57-L71)
- Commit: [59c995a](https://github.com/cosmobobak/viridithas/commit/59c995aa1e585cca51c02ee27db3d13a7398b51c)

**Berserk**: [`jhonnold/berserk@1d2354d`](https://github.com/jhonnold/berserk/tree/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49)
- [src/tune.h](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h), [src/makefile:12-16](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/makefile#L12-L16), [src/uci.c:465-500](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/uci.c#L465-L500)
- Commits: [e4a2148](https://github.com/jhonnold/berserk/commit/e4a2148e0b7d9e88984ecb786128809fd8bb62d6), [787b2e1](https://github.com/jhonnold/berserk/commit/787b2e1bfa5a363860f1a4ed52c67910188ec681), [ad9cc18](https://github.com/jhonnold/berserk/commit/ad9cc18191)

**Alexandria**: [`PGG106/Alexandria@1c48e41`](https://github.com/PGG106/Alexandria/tree/1c48e415cf55ab6a7579ae323ec800da28a0b044)
- [src/tune.h](https://github.com/PGG106/Alexandria/blob/1c48e415cf55ab6a7579ae323ec800da28a0b044/src/tune.h)
- Commit: [a4ff995](https://github.com/PGG106/Alexandria/commit/a4ff99549d)

**Obsidian**: [`gab8192/Obsidian@720614a`](https://github.com/gab8192/Obsidian/tree/720614a61ed6201037516d555f0bd2f7e5aeae3c)
- [src/tuning.h](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/tuning.h), [src/tuning.cpp:32-44](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/tuning.cpp#L32-L44), [src/search.cpp:19-74](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/search.cpp#L19-L74)

**Weather Factory**: [`jnlt3/weather-factory@19b4805`](https://github.com/jnlt3/weather-factory/tree/19b4805c9a2372955c29666118070269f34aa2eb)
- [README.md](https://github.com/jnlt3/weather-factory/blob/19b4805c9a2372955c29666118070269f34aa2eb/README.md), [spsa.py](https://github.com/jnlt3/weather-factory/blob/19b4805c9a2372955c29666118070269f34aa2eb/spsa.py), [spsa.json](https://github.com/jnlt3/weather-factory/blob/19b4805c9a2372955c29666118070269f34aa2eb/spsa.json)

**mudskipper** at `7ac9fa8`
- [src/search/worker.rs](../../src/search/worker.rs), [src/search/reduction.rs](../../src/search/reduction.rs), [src/search/history.rs](../../src/search/history.rs), [src/search/ordering.rs](../../src/search/ordering.rs), [src/search/time.rs](../../src/search/time.rs), [src/chess/board/see.rs](../../src/chess/board/see.rs), [src/eval/network.rs](../../src/eval/network.rs), [src/uci/options.rs](../../src/uci/options.rs), [src/tools/bench.rs](../../src/tools/bench.rs)
- Issues and PRs: #58, #76, #82 / PR #89 (scale check), #83 / PR #90 (gen1-256), #84
