# NNUE data generation and training for a first network

Research for generating training data with mudskipper's own search and training its first network with bullet. Written 2026-11-19 against mudskipper at `f95fa5d`. It follows [nnue-inference.md](nnue-inference.md), which fixes the network the trainer must produce: `(768 → 256)x2 → 1`, perspective, SCReLU, QA = 255, QB = 64, SCALE = 400, saved with rounding as little-endian `i16`, embedded via `include_bytes!(env!("EVALFILE"))`.

Each claim links to source at a pinned commit, or to a crate's published source. Anything I could not check in source is marked *unverified*. Time and throughput figures are **estimates**, with the arithmetic shown. Section 6 gives the step-by-step pipeline.

Pinned sources (see the [Sources](#7-sources) list):

| Source | Commit | What it is |
|---|---|---|
| bullet | `5d70177` (2026-11-05) | Trainer, `bullet-utils`, docs, examples. Same SHA as nnue-inference.md |
| bulletformat | `eddd9a2` (2026-09-14) | `ChessBoard`, version 1.8.0, the version bullet pins |
| viriformat (HEAD) | `0238b31` (2026-10-03) | Format spec (README). Version **3.0.0** |
| viriformat 2.0.1 (crates.io) | crate records git `5967bba` | The version bullet actually pins |
| akimbo (HEAD) | `f7dd767` (2025-02-07) | No datagen any more (see below) |
| akimbo v1.0.0 | `3bfea08` (2024-03-26) | Last own-data release, `datagen` feature |
| akimbo v0.6.0 | `ca0345e` (2023-09-24) | First NNUE release, `768 → 256x2 → 1` |
| Viridithas | `4609ff2` (2026-11-17) | `datagen` feature, viriformat writer. Same SHA as nnue-inference.md |
| Stormphrax | `d1468d9` (2026-08-29) | C++, datagen always built |
| Carp | `7d5065b` (2025-01-18) | Not re-cloned. Citations reused from nnue-inference.md |

**Where things live now:**

- `bullet-utils` is the workspace crate at [`crates/utils`](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils), built with `cargo b -r --package bullet-utils` ([2-getting-started.md:21-31](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/2-getting-started.md#L21-L31)).
- `bulletformat` and `viriformat` are separate repos, [`jw1912/bulletformat`](https://github.com/jw1912/bulletformat/tree/eddd9a2d77dbc1391d68f23ba524657e91bd9a73) and [`cosmobobak/viriformat`](https://github.com/cosmobobak/viriformat/tree/0238b315ea620acbf3b4d0fd8b61261efa59b0ae). bullet takes both from crates.io: `bulletformat = "1.8.0"` and `viriformat = "2.0.1"` ([bullet_lib/Cargo.toml:16-21](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/Cargo.toml#L16-L21); [utils/Cargo.toml:13-17](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/Cargo.toml#L13-L17)).
- **viriformat version mismatch.** The repo HEAD is 3.0.0 ([Cargo.toml:3](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/Cargo.toml#L3)). I downloaded and diffed the 2.0.1 crate. 3.0.0 adds skipping of "reserved extension" records ([README.md:63-75](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L63-L75)) and a `material_count_filtered` filter option ([dataformat.rs:83-88, 212-218](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/src/dataformat.rs#L83-L88)). The game encoding is unchanged. Citations to 2.0.1 point at [docs.rs's source view](https://docs.rs/crate/viriformat/2.0.1/source/).
- **akimbo HEAD has no datagen.** Its README says that up to v1.0.0 all data was self-generated, and that it "now uses data produced by Leela Chess Zero" ([README.md:16-20](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/README.md#L16-L20)). So I inspected the v1.0.0 and v0.6.0 tags.

**Checks against nnue-inference.md:**

- **The architecture and save format match.** bullet's simple example is exactly the `(768 → N)x2 → 1` SCReLU network with QA 255, QB 64, `.round()` and `eval_scale` 400 that the note pins.
- **One open point is now resolved.** The note left unverified whether `ChessBoard` is stored stm-relative with pawn = 0. It is, and section 3 has the proof. The engine-side index formulas in that note are correct.
- **One small doc gap in bullet.** `docs/4-saved-networks.md` lists only `optimiser_state/`, `quantised.bin` and `raw.bin` in a checkpoint ([:3-10](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L3-L10)). The code also writes `log.txt` ([value.rs:197-202](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L197-L202)).

Terms follow [GLOSSARY.md](../../GLOSSARY.md). A **noisy move** is a capture or promotion. "Filtered" means a position left out of training. "Soft node limit" means no new iteration starts once it is passed. "Hard node limit" means the search stops mid-iteration.

## 1. Summary

- **Datagen recipe (all three engines).** One single-threaded search per game, with as many games in parallel as there are cores. Openings are 8–9 random plies from startpos or a DFRC position. Each move gets a soft node limit and a much larger hard one. Games end on mate, draw rules, or adjudication on consecutive extreme or near-zero scores. Soft limits: akimbo 5,000 nodes, Stormphrax 24,000, Viridithas 25,000. Viridithas and Stormphrax also run a verification search on the opening and throw away lopsided ones.
- **Filtering moved to load time.** akimbo filtered at datagen, keeping only positions not in check whose best move is not a capture. Viridithas and Stormphrax's viriformat writers store *every* position, and the filter runs at load time: viriformat's `Filter` defaults to ply ≥ 16, ≥ 4 pieces, no noisy best move, not in check.
- **Formats.** bulletformat `ChessBoard` takes 32 bytes per position: stm-relative, no move, no castling. viriformat takes a 32-byte header per game plus 4 bytes per move, about 4.3 bytes per position (estimate). bullet's docs recommend viriformat. Its loader filters and shuffles inside a RAM buffer. bullet's `ChessBoard` loader reads sequentially and **never shuffles**, so that data must be shuffled first.
- **`bullet-utils shuffle` works within a RAM budget.** It is external: above the `-m` budget it splits the file into shuffled chunks of at most `-m` MiB, then randomly merges them. Peak RAM ≈ `min(file size, -m)`. `interleave` streams and uses almost no RAM. 300M positions (9.6 GB) shuffle safely with `-m 8192` on 32 GB.
- **Trainer.** Build with `--features cuda` and `CUDA_PATH` set. bullet has no CPU backend; without a GPU feature it builds a mock device. Take the config from `examples/progression/1_simple.rs`: batch 16,384, 6,104 batches per superbatch (≈ 100M positions), cosine LR from 1e-3 to 2.4e-6, WDL blend 0.75, `eval_scale` 400, save every 10 superbatches. Change only the hidden size, 128 → 256. bullet's validation-set support is not implemented, so the loss is training loss only.
- **Bootstrapping.** akimbo and Viridithas both went from tuned HCE data to a first network. akimbo's first network was exactly `768 → 256x2 → 1`, and CCRL 40/15 went from 3026 to 3335. Viridithas's first network was "much weaker than the HCE" until the same data was **shuffled**. Carp's first network was N = 256 on 100M depth-8 positions, +518 over its HCE. Stormphrax started from random weights.
- **Recommendation.** Add a `datagen` feature with **no dependencies**: a hand-written viriformat writer of about 100 lines (the spec is short), the std library's threads and a xorshift RNG, so ADR-0005 needs no change. Give the search a soft node limit. Generate games at a 10k soft / 80k hard node limit, 32 games in parallel, 8 MB TT each. Target a 15–20M-position pilot, about 2 hours (estimate), then 100M usable positions for the first real network, about 13 hours (estimate). Splat to bulletformat with the viriformat filter, shuffle with `-m 8192`, and train `1_simple` at N = 256. Validate, then SPRT against PeSTO.

## 2. Question 1: self-play datagen parameters

### Side by side

| | akimbo v0.6.0 (first net) | akimbo v1.0.0 | Viridithas HEAD | Stormphrax HEAD |
|---|---|---|---|---|
| Build | separate `datagen` crate in the workspace ([datagen/Cargo.toml](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/Cargo.toml)) | `datagen = ["bulletformat"]` feature, `make datagen` ([Cargo.toml:11-16](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/Cargo.toml#L11-L16), [Makefile:16-17](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/Makefile#L16-L17)) | `datagen` feature pulling in `rand, serde, toml, num_cpus, chrono, ctrlc, bulletformat` ([Cargo.toml:22-30, 40-47](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml#L22-L47)) | always built; `stormphrax datagen <marlinformat/viriformat/fen> <standard/dfrc> <path> [threads] [syzygy]` ([CMakeLists.txt:54-62](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/CMakeLists.txt#L54-L62), [main.cpp:43-78](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/main.cpp#L43-L78)) |
| Openings | startpos + 8 or 9 random legal moves ([thread.rs:105-108](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L105-L108)) | random line from `dfrc.epd`, + 8 or 9 random moves ([main.rs:21-25](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/main.rs#L21-L25), [datagen.rs:197-229](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L197-L229)) | DFRC by default (`generate_dfrc: true`), or classical or book; 8 or 9 random moves, preferring SEE ≥ −1000 (8 tries, then any move) ([datagen.rs:52-59, 112-123, 158-232](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L158-L232)) | startpos or a random DFRC index, + 8 or 9 uniformly random moves ([datagen.cpp:146-171](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L146-L171)) |
| Opening check | none | none | 50k soft-node search; discard if \|eval\| > 1000 ([datagen.rs:517-531](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L517-L531)) | depth 10 / 500k hard nodes; discard if \|normalised score\| > 500 ([datagen.cpp:76-82, 179-193](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L179-L193)) |
| Per-move limit | soft 5,000 nodes ([main.rs:13](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/main.rs#L13)), hard 1M nodes / 10 s ([thread.rs:36-40](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L36-L40)) | soft 5,000, hard 1M nodes, 10 s ([datagen.rs:12-17](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L12-L17)) | soft 25,000 (default), hard 8 × soft ([datagen.rs:121, 471-474](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L471-L474)) | soft 24,000, hard 1,000,000 ([datagen.cpp:79-80](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L79-L80)); a game is discarded after 3 moves hit the hard limit ([:92, 282-284](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L282-L284)) |
| Win adjudication | \|score\| > 1000 once ([thread.rs:132-137](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L132-L137)) | \|score\| > 3000 once ([datagen.rs:13, 239-244](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L239-L244)) | \|score\| ≥ 2500 for 4 plies in a row, or a mate/TB score ([datagen.rs:579-610](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L579-L610)) | normalised \|score\| > 1250 for 5 plies, or a decisive score ([datagen.cpp:84-89, 224-253](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L224-L253)) |
| Draw adjudication | none; draw rules only ([thread.rs:148-162](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L148-L162)) | none; stalemate, 50-move, repetition ([datagen.rs:264-278](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L264-L278)) | \|score\| ≤ 4 for 12 plies in a row ([datagen.rs:583-600](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L583-L600)) | from ply 70 on, \|normalised score\| < 10 for 10 plies ([datagen.cpp:85-90, 235-251](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L235-L251)) |
| Position filtering | **at datagen**: keeps a position only if not in check and the best move is not a capture ([thread.rs:139](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L139)); also drops the last 8 positions of drawn games ([:57-64](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L57-L64)) | at datagen: not in check, best move not a capture ([datagen.rs:246-256](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L246-L256)) | **at load time**: every move and score is stored ([datagen.rs:572-578, 621](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L572-L578)); filtering is done by viriformat's `Filter` when splatting or loading (below) | marlinformat output filters in-check / noisy best move at datagen ([datagen.cpp:255](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L255); [marlinformat.cpp:31-36](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/marlinformat.cpp#L31-L36)); viriformat output ignores the flag and stores all ([viriformat.cpp:33-51](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/viriformat.cpp#L33-L51)) |
| Scores stored | white-relative search score ([thread.rs:139-142](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L139-L142)) | white-relative, `i16` ([datagen.rs:247-255](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L247-L255)) | search score per move ([datagen.rs:572-578](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L572-L578)) | white-relative raw score, \|s\| ≤ 2 clamped to 0 ([datagen.cpp:286-287](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L286-L287); [search.cpp:237-238](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L237-L238)) |
| Format | text `<fen> \| <result>` ([thread.rs:57-64](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L57-L64)) *(the score is presumably inside the `fen` string; unverified)* | bulletformat `ChessBoard` ([datagen.rs:150-153](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L150-L153)) | viriformat, one `thread_{id}.bin` per worker ([datagen.rs:482](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L482)) | marlinformat, viriformat or FEN ([datagen.cpp:338-345](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L338-L345)) |
| Threads | `threads` arg, one game per thread ([main.rs:18-29](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/main.rs#L18-L29)) | same ([datagen.rs:43-54](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L43-L54)) | `num_threads` workers, each with **one** search thread ([datagen.rs:424-433](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L424-L433)); warns against exceeding logical cores ([:687](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L687)) | `threads` arg, one searcher per thread, NUMA-bound ([datagen.cpp:97-99, 377-388](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L377-L388)) |
| Hash per worker | 8 MB, TT and history reset per game ([thread.rs:25, 51, 72-75](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs#L72-L75)) | 8 MB, TT cleared per game ([datagen.rs:51, 129-140](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L129-L140)) | **two** 4 MB TTs per game, one per colour, so the players can't "see into each other's minds"; cleared per game ([datagen.rs:434-445, 494-498](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L434-L445)) | `kTtSize = 16` (*unverified:* the unit, presumably MiB), `newGame()` per game ([datagen.cpp:74, 123, 131-135](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp#L131-L135)) |
| Search changes in datagen | — | futility and late move pruning and eval scaling compiled out ([search.rs:502-509](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/search.rs#L502-L509); [position.rs:256-271](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/position.rs#L256-L271)) | the soft node limit exists only under `datagen` ([timemgmt.rs:104-113](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/timemgmt.rs#L104-L113)) | stops on a decisive score; skips some PV-node pruning ([search.cpp:501-503, 1024](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L501-L503)) |

**Soft limit semantics.** All three check the soft limit only between iterations: akimbo at [search.rs:135-138](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/search.rs#L135-L138), Viridithas's `is_past_opt_time` at [timemgmt.rs:106-112](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/timemgmt.rs#L106-L112), and Stormphrax's `stopSoft` versus `stopHard` at [limit.cpp:165-188](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/limit.cpp#L165-L188). Each move therefore searches to whatever depth the soft budget reaches, and the hard limit only catches explosions.

**viriformat's load-time filter.** `Filter::default()` sets `min_ply` 16, `min_pieces` 4, `max_eval` 31339, `filter_tactical` and `filter_check` on, and random skipping off ([2.0.1 dataformat.rs:27-60, Default after](https://docs.rs/crate/viriformat/2.0.1/source/src/dataformat.rs); HEAD [dataformat.rs:91-119, 178-239](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/src/dataformat.rs#L178-L239)). Two details:

- "Tactical" is judged on the move **played from** the position, the self-play best move: promotion, en passant or capture ([2.0.1 board/mod.rs:1164-1166](https://docs.rs/crate/viriformat/2.0.1/source/src/chess/board/mod.rs)). That is the glossary's noisy move.
- `ply` comes from the stored fullmove counter: `ply = (fullmove − 1)·2 + stm` ([2.0.1 board/mod.rs:204-205](https://docs.rs/crate/viriformat/2.0.1/source/src/chess/board/mod.rs)). **A writer must store a real fullmove number ≥ 1.** A zero underflows. bullet's advanced example uses a stricter filter: castling filtered too, `max_eval` 10000, an eval-versus-result consistency cut of 2500, and 15% random skipping ([advanced/filter.rs:46-76](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/filter.rs#L46-L76)).

### What mudskipper already has

- **Limits.** `Limits` has `depth`, `nodes` and `time` ([worker.rs:107-116](../../src/search/worker.rs)). The node limit is **hard only**: `should_stop` ends the search mid-iteration once `nodes >= limit`, even before the first iteration completes ([worker.rs:701-709](../../src/search/worker.rs)). The iterative-deepening loop checks a *soft* time limit between iterations ([worker.rs:229-231](../../src/search/worker.rs)). A soft node limit is the same check on `self.nodes`. That is the one search change datagen needs: a `soft_nodes: Option<u64>` checked beside `soft_time_is_up()`.
- **Threads.** Search is single-threaded. `Threads` is accepted but "Only one is used for now" ([options.rs:41-47](../../src/uci/options.rs)). `Worker` is already designed to be replicated per thread, with only the transposition table shared ([search.rs:1-5](../../src/search.rs)). A `TranspositionTable` is an `Arc` handle, so each datagen thread just makes its own ([tt.rs:204-211](../../src/search/tt.rs)). One game per OS thread with no Lazy SMP is exactly what all three references do.
- **State reset.** History is kept between searches, "so a fresh worker is needed to forget it" ([worker.rs:141](../../src/search/worker.rs)). Datagen should create a new `Worker` and `tt.clear()` per game, as `ucinewgame` does ([uci.rs:319-337](../../src/uci.rs)).
- **Draw rules and moves.** `is_fifty_move_draw`, `has_insufficient_material` ([draw.rs:18, 28](../../src/chess/board/draw.rs)), `KeyHistory::repetitions` ([key_history.rs:39](../../src/chess/key_history.rs)), `Game::play` ([game.rs:49](../../src/chess/game.rs)), `legal_moves` ([movegen.rs:61](../../src/chess/board/movegen.rs)), `in_check` ([board.rs:159](../../src/chess/board.rs)) and `Move::is_noisy` ([moves.rs:184](../../src/chess/moves.rs)) are all there.
- **Castling encoding.** Moves are encoded internally as king-takes-rook (ADR-0001), which is exactly viriformat's castling encoding ([README.md:36](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L36)).
- **No DFRC generator.** There is no DFRC-index position generator. Classical openings with random plies need nothing new, and that is what akimbo's first-net data used.
- **Command line.** `main.rs` already dispatches `bench [depth]` ([main.rs:14-24](../../src/main.rs)). A `datagen` subcommand fits the same `match`.
- **Scores.** The search score is reported as `Score`, with mate kept separate ([score.rs:55-70](../../src/search/score.rs)). Positions in a mate line should end the game, not be recorded with mate scores. Mate scores sit above `MATE_BOUND = 31872` ([score.rs:10-13](../../src/search/score.rs)), which viriformat's default `max_eval` of 31339 would filter anyway.

### How many positions, and how long on an i9-13900K

**Evidence for the count:**

- Carp's first network, N = 256 on **100M** depth-8 self-play positions, gained **+518** over its HCE ([nnue_history.txt:4-5](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L4-L5), via nnue-inference.md).
- akimbo's first network was `768 → 256x2 → 1` ([v0.6.0 README.md:37](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/README.md#L37)), trained on "the final HCE dataset" ([:20-22](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/README.md#L20-L22)), generated at 5,000 soft nodes. *Unverified:* the dataset size, which no file in the repo states.
- In Viridithas's history, 80M fresh positions alone lost −87.6 Elo to the previous network's larger dataset, but helped once merged into it ([networkhistory.txt:108-117](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L108-L117)). That compares networks against networks, not against an HCE.
- bullet's simple schedule is sized for this scale. `6104 × 16384 = 100,007,936`, so one superbatch is about **100M positions**, and 40 superbatches is 40 passes over a 100M dataset ([1_simple.rs:47-52](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L47-L52)).

So **100M usable positions** is the evidence-backed target for a first N = 256 network. nnue-inference.md already says to drop to N = 128 if the data falls well short. A 15–20M pilot is enough to exercise the pipeline and should already beat PeSTO (*unverified;* no source tested so small a first network).

**Wall-clock estimate.** All of it is an estimate except the measured bench line.

1. **Measured:** `mudskipper bench` (depth 8, release, portable build, `f95fa5d`) gives **726,262 nodes over 50 positions at 2,999,843 nps** on one thread of an i9-9900K VM. That is about **14,500 nodes for a depth-8 search**, which puts Carp's "depth 8" between akimbo's 5k and Viridithas's 25k soft limit.
2. **Assumed throughput:** with 32 single-threaded games on a 13900K (8 P-cores with HT and 16 E-cores), I assume **≈ 50 Mnps in total**, with a plausible range of 40–70. That is 3–3.5 Mnps per P-core-equivalent, × (8 × 1.25 HT gain + 16 × 0.55 E-core ratio ≈ 19). *Unverified:* every factor here. Measure it by running the datagen for a minute and reading the positions-per-second line.
3. **Assumed nodes per move:** a soft limit S lets the iteration that crosses S finish. I assume about 1.5·S per move on average. *Unverified;* datagen should log the mean.
4. **Assumed retention:** the viriformat default filter (ply ≥ 16, no noisy best move, not in check) keeps **≈ 65%** of stored positions. *Unverified;* measure it as the splatted file size ÷ 32 against the game record count.

| Soft limit S | Nodes per move (≈1.5·S) | Positions stored per second (50 Mnps ÷ nodes per move) | Stored per hour | Usable per hour (×0.65) | Pilot, 15M usable | 100M usable |
|---|---|---|---|---|---|---|
| 5,000 (akimbo) | 7,500 | 6,667 | 24.0M | 15.6M | ~1.0 h | ~6.4 h |
| **10,000 (recommended)** | 15,000 | 3,333 | 12.0M | 7.8M | **~1.9 h** | **~12.8 h** |
| 25,000 (Viridithas) | 37,500 | 1,333 | 4.8M | 3.1M | ~4.8 h | ~32 h |

The general form is `hours = target / (aggregate_nps × 3600 / (1.5·S) × retention)`. Replace 50 Mnps and 0.65 with measured values once the pilot runs. Disk: viriformat at about 4.3 bytes per position (section 3) is about 0.65 GB for 150M stored positions. RAM: 32 workers × 8 MB TT ≈ 256 MB.

S = 10,000 is my choice, not a source's. It is near mudskipper's own depth-8 node count, so near Carp's first-network data, and twice akimbo's first-network budget. Stronger labels matter more for an engine whose evaluation is only PeSTO.

## 3. Question 2: data formats and a low-RAM pipeline

### bulletformat `ChessBoard`: 32 bytes per position

- **Layout:** `occ: u64, pcs: [u8; 16], score: i16, result: u8, ksq: u8, opp_ksq: u8, extra: [u8; 3]`, `#[repr(C)]`, asserted to be 32 bytes ([chess.rs:9-21](https://github.com/jw1912/bulletformat/blob/eddd9a2d77dbc1391d68f23ba524657e91bd9a73/src/chess.rs#L9-L21)).
- **Perspective.** It is **stm-relative**. For Black to move, `from_raw` byte-swaps every bitboard, which mirrors the ranks, swaps the colours, and negates the score and result ([chess.rs:105-114](https://github.com/jw1912/bulletformat/blob/eddd9a2d77dbc1391d68f23ba524657e91bd9a73/src/chess.rs#L105-L114)). The text parser does the same with `piece ^= 8; square ^= 56` ([:184-190, 241-244](https://github.com/jw1912/bulletformat/blob/eddd9a2d77dbc1391d68f23ba524657e91bd9a73/src/chess.rs#L184-L190)). Pieces are 4-bit, `colour << 3 | type` with pawn = 0 … king = 5 ([:126-135](https://github.com/jw1912/bulletformat/blob/eddd9a2d77dbc1391d68f23ba524657e91bd9a73/src/chess.rs#L126-L135)). bullet's docs confirm it "throws away information such as the side-to-move (the record is stored stm-relative), halfmove counter, castling rights" ([3-data.md:20-26](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/3-data.md#L20-L26)).
- **Feature mapping.** `Chess768` computes `stm = [0,384][c] + 64·pc + sq` and `ntm = [384,0][c] + 64·pc + (sq^56)` in those stm-relative coordinates ([chess768.rs:20-30](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/game/inputs/chess768.rs#L20-L30)). Translated back to absolute squares, the stm feature is `colour·384 + type·64 + sq` when White is to move, and `(1−colour)·384 + type·64 + (sq^56)` when Black is. That is exactly nnue-inference.md's `white_index` and `black_index`, with the stm accumulator from the side to move's own perspective. **This closes that note's open point.**
- **Text conversion.** Lines of the form `<FEN> | <score> | <result>`, with the score white-relative in centipawns and the result 1.0, 0.5 or 0.0 ([3-data.md:28-31](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/3-data.md#L28-L31)), convert with `bullet-utils convert --from text` ([convert.rs:16-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/convert.rs#L16-L39)).

### viriformat `Game`: variable length

- **Spec.** All integers are little-endian ([README.md:5](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L5)). A game is a 32-byte marlinformat `PackedBoard`, then `(u16 move, i16 white-relative score)` pairs, then four zero bytes ([:11-41](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L11-L41)).
- **PackedBoard.** It stores occupancy and 4-bit pieces, with castling as an unmoved-rook piece type 6. It also stores the stm and ep byte, halfmove, fullmove, a score, a result byte (0 = black win, 1 = draw, 2 = white win) and one spare byte ([:13-31](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L13-L31)).
- **Move.** 6 bits from, 6 bits to, 2 bits promotion piece, 2 bits type (1 = en passant, 2 = castle, 3 = promotion), with king-takes-rook castling ([:33-39](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L33-L39)). Stormphrax's writer shows the bit placement: type `0x4000` en passant, `0x8000` castle, `0xC000` promotion ([viriformat.cpp:36-50](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/viriformat.cpp#L36-L50)).
- **Writer.** `serialise_into` writes the header, each move and score `to_le_bytes`, then the terminator ([2.0.1 dataformat.rs:275-283](https://docs.rs/crate/viriformat/2.0.1/source/src/dataformat.rs)). Stormphrax's C++ writer is about 30 lines ([viriformat.cpp:28-63](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/viriformat.cpp#L28-L63)).
- **Footprint.** Each game costs `32 + 4·moves + 4` bytes. At 150 plies per game (*estimate*), that is 636 / 150 ≈ **4.2 bytes per stored position**, about 1/7.5 of `ChessBoard`. Per *usable* position after 65% retention it is about 6.5 bytes. bullet-utils' `viribinpack splat` prints "Bytes per position" ([splat.rs:63-67](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/splat.rs#L63-L67)).
- **bullet's recommendation:** "I would recommend using Viriformat binpacks". It recommends `ChessBoard` only "if bottlenecked by data loading speed" ([3-data.md:12-24](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/3-data.md#L12-L24)).

### Getting from viriformat to what bullet trains on

There are two routes.

**A. Train directly from viriformat with `ViriBinpackLoader`** ([simple.rs:74-87](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L74-L87)):

- **Reading and filtering.** One thread reads whole games ([viribinpack.rs:147-174](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/loader/viribinpack.rs#L147-L174)). `threads` workers splat them into `ChessBoard`s through the filter ([:242-298](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/loader/viribinpack.rs#L242-L298)).
- **Shuffling.** Positions collect in a buffer of `buffer_size_mb × 2^20 / 32 / 2` entries, which is `buffer_size_mb / 2` MiB. That buffer is Fisher-Yates shuffled and handed to the trainer ([:51, 114-132, 300-307](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/loader/viribinpack.rs#L114-L132)). So **this loader shuffles for itself, but only within a window**. With 1024 MB (the example) the window is 512 MiB ≈ 16.7M positions, drawn from consecutive games.
- **Peak RAM** ≈ one buffer being filled plus one being consumed ≈ `buffer_size_mb`, plus read-ahead of `4 × 8192·threads` raw games (*estimate:* tens of MB). Filters run again every epoch, which costs CPU, not disk.
- **Multiple files.** `new_interleave_multiple` mixes files 16 games at a time in proportion to their sizes ([:58-65, 176-240](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/loader/viribinpack.rs#L176-L240)). `bullet-utils viribinpack interleave` does the same offline at game granularity ([viribinpack/interleave.rs:54-81](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/interleave.rs#L54-L81)), streaming, and checks that the output size equals the sum of the inputs ([:88-92](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/interleave.rs#L88-L92)).

**B. Splat to `ChessBoard`, shuffle, then train with `DirectSequentialDataLoader`:**

- **Splat.** `bullet-utils viribinpack splat <in.vf> <out.bin> <filter.toml>` writes each passing position as 32 raw bytes ([splat.rs:12-61](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/splat.rs#L12-L61)).
- **Two traps in `splat`:**
  - The filter path is a **required** positional argument despite its "Optional" doc comment ([:20-21](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/splat.rs#L20-L21)). Any read or parse failure **silently falls back to `Filter::UNRESTRICTED`** (`unwrap_or`, [:38](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/splat.rs#L38)), so a typo in the path disables all filtering. TOML fields default individually (`#[serde(default)]`, [2.0.1 dataformat.rs:25](https://docs.rs/crate/viriformat/2.0.1/source/src/dataformat.rs)), so a file containing only `min_ply = 16` gets the default filter for everything else.
  - The printed "Positions" counts **every** move, filtered or not ([:44-45](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/splat.rs#L44-L45)). Check the output size ÷ 32 instead.
- **The direct loader never shuffles.** It reads files front to back in 128 MiB chunks, forever ([fixed_size.rs:42-107](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/reader/fixed_size.rs#L42-L107); wrapper at [direct.rs:40-47](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/loader/direct.rs#L40-L47)). Splat output is in game order, so it **must** be shuffled. Viridithas's second network fixed the first one's problems simply because "data was shuffled" ([networkhistory.txt:7-8](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L7-L8)).

### How `shuffle` and `interleave` work

**`bullet-utils shuffle -i <in> -o <out> -m <MiB>`** ([shuffle.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs)):

1. If the input is no larger than `-m` MiB, it reads the **whole file** into one `Vec`, Fisher-Yates shuffles it in place and writes it out ([:40-46](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L40-L46)). Peak RAM is the file size.
2. Otherwise it creates `<out>.tmp/`, which must **not** already exist (`fs::create_dir`, [:49-53](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L49-L53)). It splits the input into `k = max(ceil(size / -m), 4)` contiguous parts ([:54-55](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L54-L55)). It reads each part into a freshly allocated buffer of at most `-m` MiB, shuffles it in memory and writes it to `part_i.bin`, one at a time ([:83-132](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L83-L132)). It then **interleaves** the parts into the output and deletes the temp dir ([:67-75](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L67-L75)).
3. **Peak RAM ≈ `min(file size, -m MiB)`** plus buffers. **Peak extra disk** = one input-sized temp dir plus the output.
4. The shuffle swaps byte by byte, 32 swaps per element ([:135-148](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L135-L148)). *Unverified:* its speed.

**`bullet-utils interleave <a> <b> … -o <out>`** ([interleave.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/interleave.rs)) opens a `BufReader` per input (8 KiB default buffer) ([:31-40](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/interleave.rs#L31-L40)). For each output record it picks an input with probability proportional to the records it has left, and copies 32 bytes ([:45-69](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/interleave.rs#L45-L69)). RAM use is constant, a few KiB per input. Record size is hard-coded to 32 ([:21](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/interleave.rs#L21)), so it is `ChessBoard`-only. Use `viribinpack interleave` for viriformat.

**Is the chunked shuffle a full shuffle?** Yes, by my reasoning, not stated in the source. Each part is uniformly permuted, and the merge picks the next part with probability proportional to its remaining count, which yields a uniformly random interleaving. Together that gives a uniformly random permutation of the whole file, so the RAM cap does not make the shuffle worse.

### Shuffling 100–300M positions under 32 GB on Windows 11

| Positions | `ChessBoard` file | `shuffle -m 8192` | Peak RAM | Free disk needed (temp + output) |
|---|---|---|---|---|
| 100M | 3.2 GB | in-memory path (≤ 8 GiB) | ≈ 3.2 GB | 3.2 GB (output only) |
| 200M | 6.4 GB | in-memory path | ≈ 6.4 GB | 6.4 GB |
| 300M | 9.6 GB | chunked, k = 4 parts of 2.4 GB | ≈ 2.4 GB per part (≤ 8 GiB cap) | 19.2 GB |

Arithmetic: 32 bytes × N. Note that `k` is never below 4.

The safe workflow follows bullet's own order ([3-data.md:3-8](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/3-data.md#L3-L8)): splat each viriformat file, shuffle each with `-m 8192`, then `interleave` the shuffled files. Every step stays at or under 8 GiB, leaving more than 20 GB for Windows and the trainer. Writing to the same path as the input is refused ([shuffle.rs:151-160](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/shuffle.rs#L151-L160)). bullet-utils builds without CUDA ([2-getting-started.md:31](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/2-getting-started.md#L31)). Nothing in `shuffle.rs` or `interleave.rs` is platform-specific.

## 4. Question 3: training a first network with bullet on an RTX 4090

### Backend and build

- **Features.** `bullet_lib` has `cuda`, `rocm` and `metal` ([bullet_lib/Cargo.toml:23-26](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/Cargo.toml#L23-L26)). With none of them the execution context is `MockGpu` ([nn.rs:15-16](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/nn.rs#L15-L16); [trainer run.rs:24-25](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/run.rs#L24-L25)). There is no CPU training backend. For an RTX 4090: `cargo r -r --example <name> --features cuda`.
- **CUDA prerequisites.** Install the CUDA Toolkit, "as recent a version as possible", and set `CUDA_PATH` to the directory containing `bin`, `lib` and `include` ([2-getting-started.md:35-43](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/2-getting-started.md#L35-L43)). The build script panics without `CUDA_PATH`, searches `lib/x64` and `lib` on Windows, and links `cuda`, `cudart`, `nvrtc` and `cublas` ([gpu/build.rs:3-41](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/gpu/build.rs#L3-L41)). *Unverified:* any further Windows requirement, such as the MSVC toolchain or a minimum driver. bullet's docs list none.
- **Project setup.** Either edit an example in a bullet checkout (examples must be registered in `bullet_lib/Cargo.toml`, [README.md](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/README.md)), or depend on `bullet_lib` by git ([2-getting-started.md:9-12](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/2-getting-started.md#L9-L12)). Pin `rev = "5d70177…"` for reproducibility.

### Schedule knobs, from the examples

| Knob | `examples/simple.rs` | `examples/progression/1_simple.rs` | advanced (multi-stage, not a first net) |
|---|---|---|---|
| Hidden size | 128 ([:19](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L19)) | 128 ([:14](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L14)) | 768 + more layers |
| Batch size | 16,384 ([:62](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L62)) | 16,384 ([:48](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L48)) | 131,072 ([main.rs:170](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/main.rs#L170)) |
| Batches per superbatch | 6,104 ([:63](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L63)) | 6,104 ([:49](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L49)) | 763 ([main.rs:171](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/main.rs#L171)); same ≈100M per superbatch |
| Superbatches | 40 ([:65](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L65)) | 40 ([:18](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L18)) | 100 + 800 + 200 ([main.rs:37-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/main.rs#L37-L39)) |
| WDL blend | constant 0.75 ([:67](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L67)) | constant 0.75 ([:19, 53](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L53)) | 0.2 → linear 0.2–0.5 → 1.0 by stage ([main.rs:204-218](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/main.rs#L204-L218)) |
| LR | `StepLR { start: 0.001, gamma: 0.1, step: 18 }`, dropping at superbatches 19 and 37 ([:68](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L68); [lr.rs:59-72](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/trainer/schedule/lr.rs#L59-L72)) | `CosineDecayLR` 0.001 → 0.001·0.3⁵ ≈ 2.43e-6 at superbatch 40 ([:16-17, 54](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L16-L17); [lr.rs:116-136](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/trainer/schedule/lr.rs#L116-L136)) | linear warm-up to 5e-3, then linear decays |
| `eval_scale` | 400 ([:20, 60](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L60)) | 400 ([:46](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L46)) | — |
| Save rate | 10 ([:69](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L69)) | 10 ([:55](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L55)) | 32 |
| `LocalSettings` | threads 4, queue 64 ([:72](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L72)) | threads 2, queue 32 ([:58](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L58)) | read buffer 8192 MB, 8 read + 8 map threads ([main.rs:31-34](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/main.rs#L31-L34)) |

Other schedulers exist: `LinearWDL`, WDL `Warmup` and `Sequence` ([wdl.rs:31-104](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/trainer/schedule/wdl.rs#L31-L104)), and LR `DropLR`, `LinearDecayLR`, `ExponentialDecayLR`, `Warmup` and `Sequence` ([lr.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/trainer/schedule/lr.rs)).

**What the WDL blend means.** The target is `blend · result + (1 − blend) · sigmoid(score / eval_scale)` ([value.rs:103-115](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L103-L115)), so 0.75 means **75% game result**. Viridithas's measurements point lower for its data, though that was at much higher data volumes, so I treat them as a hint only:

- 90% result: −75 Elo ([networkhistory.txt:47-49](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L47-L49)).
- 25%: +16 over the previous setting ([:67-69](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L67-L69)).
- 40%: +8 ([:76-78](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L76-L78)).

**Eval scale and PeSTO.** The data's scores will be mudskipper's own search scores, in PeSTO-based centipawns, with pawn 82 in the middlegame and 94 in the endgame ([pesto.rs:10-13](../../src/eval/pesto.rs)). `eval_scale` 400 makes the float output `cp / 400`, and SCALE = 400 at inference returns it to those centipawns (nnue-inference.md §4). The blend pulls the output towards whatever scale makes `sigmoid(cp/400)` match game results. As that note suggests, fit `K` in `sigmoid(cp/K)` against results on the pilot data, and keep 400 unless the fit lands far from it. bullet also has an optional `use_win_rate_model` target with hard-coded constants 270 and 380 ([value.rs:104-107](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L104-L107)). Those are tuned to another engine's scale (*unverified* whose), so leave it off.

**Save format.** Keep `simple.rs`'s list exactly: `l0w` and `l0b` at `.round().quantise::<i16>(255)`, `l1w` at `.round().quantise::<i16>(64)`, and `l1b` at `.round().quantise::<i16>(255 * 64)` ([simple.rs:34-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L34-L39)). Optimiser `AdamW`, which clips to ±1.98 ([:28-30](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L28-L30)). Loss `sigmoid().squared_error` ([:44](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L44)). This is what nnue-inference.md §7 assumes.

**Checkpoint layout.** Every `save_rate` superbatches, and at the last, bullet writes `<output_directory>/<net_id>-<superbatch>/` ([value.rs:197-202](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L197-L202)) containing:

- `optimiser_state/`
- `raw.bin` (float, unpadded)
- `quantised.bin`, the file for `EVALFILE`. If quantisation overflows it is skipped with a message, and training continues ([save.rs:24-43](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/save.rs#L24-L43)).
- `log.txt`, with lines `superbatch,batch,loss` logged every 32 batches ([save.rs:15-22](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value/save.rs#L15-L22); [value.rs:181-193](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L181-L193)).

For N = 256, `quantised.bin` must be `(768·256 + 256 + 512 + 1) × 2 = 394,754` bytes padded to 64, which is **394,816 bytes** (my arithmetic). That gives a hard check for the engine's `const` size assertion.

**No validation loss.** `LocalSettings::test_set` exists ([settings.rs:3-29](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/trainer/settings.rs#L3-L29)), but `run` prints "Warning: Validation data not currently implemented!" ([value.rs:153-158](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L153-L158)). Overfitting has to be judged by SPRT, not by a held-out loss.

**Throughput.** *Unverified:* GPU positions per second for `768 → 256x2 → 1` on a 4090. bullet prints progress during training. `measure_max_cpu_throughput` checks whether data loading is the bottleneck ([value.rs:256-271](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L256-L271)).

### First-network config

These are `1_simple.rs` values, with my changes marked:

```rust
// Copy of examples/progression/1_simple.rs (bullet 5d70177), registered in bullet_lib/Cargo.toml.
let hl_size = 256;                                   // changed from 128 (nnue-inference.md §7)
let dataset_path = "data/gen1.shuffled.bin";         // ChessBoard, pre-shuffled (route B)
let initial_lr = 0.001;                              // 1_simple.rs:16
let final_lr = 0.001 * 0.3f32.powi(5);               // 1_simple.rs:17
let superbatches = 40;                               // 1_simple.rs:18
let wdl_proportion = 0.75;                           // 1_simple.rs:19 (pilot A/B against 0.4, below)
// .save_format: l0w/l0b round+quantise 255, l1w 64, l1b 255*64        1_simple.rs:25-30
// .build: l0 = affine(768, hl), l1 = affine(2*hl, 1), screlu, concat  1_simple.rs:32-42
TrainingSchedule {
    net_id: "gen1".into(),
    eval_scale: 400.0,                               // 1_simple.rs:46; = SCALE
    steps: TrainingSteps { batch_size: 16_384, batches_per_superbatch: 6104,
                           start_superbatch: 1, end_superbatch: superbatches },  // :47-52
    wdl_scheduler: wdl::ConstantWDL { value: wdl_proportion },                   // :53
    lr_scheduler: lr::CosineDecayLR { initial_lr, final_lr, final_superbatch: superbatches }, // :54
    save_rate: 10,                                                               // :55
};
LocalSettings { threads: 4, test_set: None, output_directory: "checkpoints", batch_queue_size: 64 }; // simple.rs:72
```

With 100M positions, that is 40 passes. For the **pilot** (15–20M positions) keep everything but set `batches_per_superbatch: 1024` and `superbatches = 20`. That is about 16.8M samples per superbatch, roughly one pass per superbatch and 20 passes in total. These two numbers are my choice, scaled from the example's "one superbatch ≈ one pass".

## 5. Question 4: bootstrapping and validation

### How the references got from HCE to NNUE

- **akimbo.** It started from material values, then iterated self-generated data and tuning through a PST-only and then a fuller HCE. "The final HCE dataset was then used to train akimbo's first network and further data was then generated" ([README.md:16-18](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/README.md#L16-L18)). The first network shipped in v0.6.0 as `768 → 256x2 → 1` ([v0.6.0 README.md:37](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/README.md#L37)). CCRL 40/15 went from 3026 for v0.5.0, "Better HCE", to **3335** for v0.6.0, "NNUE" ([README.md:34-35](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/README.md#L34-L35)). Its datagen at the time was the 5k-node, startpos + random-moves generator described in section 2.
- **Viridithas.** It went PeSTO, then Texel-tuned HCE, then a first network trained on old self-play games "rescored with a development version of Viridithas 2.7.0 at low depth" ([README.md:51-60](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/README.md#L51-L60)). The network log shows:
  - viri0: "much weaker than the HCE".
  - viri1: the same data, but shuffled, "crushes HCE" ([networkhistory.txt:3-8](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L3-L8)).
  - viri5–10: nothing improved.
  - viri11: filtering noisy positions, +20.
  - viri12: data relabelled with viri11, +50 ([:16-23](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L16-L23)).

  Its own datagen came later, between 7.0.0 and 8.0.0 ([README.md:61-62](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/README.md#L61-L62)). It still has a relabel tool that rescores existing games with the current network ([datagen.rs:1084](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs#L1084)).
- **Stormphrax.** "NNUE evaluation trained from zero knowledge starting with random weights" ([README.md:14](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/README.md#L14)). It is a continuation of the HCE engine Polaris ([:18](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/README.md#L18)), and very early networks used Marlinflow ([:99](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/README.md#L99)). *Unverified:* its generation-by-generation history, which is not in the repo. The current network id is `net096_255_128_q6` ([network.txt](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/network.txt)).
- **Carp.** carp0000 was N = 256 on 100M depth-8 positions, +518 over its HCE. Width then grew only as data grew (nnue-inference.md §2).

For mudskipper, generation 0 is PeSTO, which is where Viridithas started. ADR-0006 already anticipates training on the engine's own search output.

### Validation before scaling up

1. **Pipeline and data checks**, before any training:
   - `bullet-utils validate` on the splatted file checks king squares and piece validity, and prints the W/D/L split ([validate.rs:14-100](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/validate.rs#L14-L100)).
   - `viribinpack count` reports games and bytes per position ([viribinpack/count.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/src/viribinpack/count.rs)).
   - Round-trip a few games through mudskipper's writer and `splat`, and check that the FENs match the games played.
2. **Loss curve.** Read `log.txt`: the loss should fall steadily and flatten as the cosine LR decays. There is no validation loss (section 4), so the pilot SPRT is the overfitting check.
3. **Float versus quantised.** In the training binary, after `run`, call `trainer.eval(fen)` for a few FENs ([value.rs:236-254](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L236-L254)). For a single-output network it returns the raw output (`eval_raw_output` maps with scale 1.0, [:211-233](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L211-L233)). *Unverified:* that the raw output is pre-sigmoid. The model's last op is the affine layer, with the sigmoid only in the loss ([simple.rs:44-55](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L44-L55)), which suggests it is. `× 400` should match mudskipper's integer evaluation of `quantised.bin` to within a few centipawns of rounding.
4. **Engine sanity tests** (suggestions, not from a source):
   - Startpos evaluates small and positive.
   - **Colour-mirror equality:** a position and its colour-flipped, rank-mirrored twin must evaluate *identically*. Under perspective inputs the twin's stm features are the original's stm features, so this is exact, not approximate.
   - The incremental accumulator equals a from-scratch refresh (nnue-inference.md §7, step 6).
   - Obvious material imbalances get the right sign.
   - Bench runs and gives a new **Bench signature**.
5. **Calibration** (suggestion): on a held-out slice of positions, bucket by the network's centipawns and compare each bucket's score rate with `sigmoid(cp / 400)`. A systematic tilt means `eval_scale` and the data's scale disagree.
6. **SPRT cadence.** Each new network plays an **SPRT** against the current best, with the pilot first:
   - Pilot versus PeSTO master.
   - First real network versus pilot (or PeSTO).
   - Generation n+1 versus generation n.

   Viridithas logs network SPRTs at `8.0+0.08s Threads=1 Hash=16MB` ([networkhistory.txt:191-192](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt#L191-L192)). A large first-network gain ends an SPRT quickly. The eval-based search margins (reverse futility, futility, aspiration window) were tuned on PeSTO, so plan follow-up SPRTs on them after the switch (nnue-inference.md §4).
7. **Generation 2.** Generate new data with the generation-1 engine, or relabel generation-1 data with the generation-1 network (Viridithas viri12, +50). Add it to the earlier data rather than replacing it (viri38 versus viri39, section 2). Widen the hidden layer only once the data supports it (Carp).

## 6. Recommended pipeline: i9-13900K, RTX 4090 (24 GB), 32 GB DDR5, Windows 11

This section describes changes; it does not make them.

### Step 1. A `datagen` feature with no new dependencies

- **Cargo.toml:** add `[features] datagen = []`. **No optional dependency.** Write the viriformat writer by hand instead of depending on `viriformat`, for these reasons:
  - The format is a 32-byte header plus 4-byte records, specified in one README ([README.md:3-41](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L3-L41)).
  - mudskipper already uses king-takes-rook castling and a1 = 0.
  - `viriformat` 2.0.1 would pull in `anyhow`, `arrayvec`, `bulletformat`, `rand`, `serde` and `toml`, plus a whole second chess board implementation ([2.0.1 Cargo.toml](https://docs.rs/crate/viriformat/2.0.1/source/Cargo.toml)). That is exactly what ADR-0005 keeps out.
  - Use a xorshift RNG like akimbo's ([datagen.rs:121-126](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L121-L126)) and `std::thread::scope`. Stop on a `stop` line from stdin, as akimbo does ([:56-64](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs#L56-L64)), rather than with `ctrlc`.
  - **ADR-0005 then needs no amendment.**
- **If a crate is preferred anyway:** `viriformat = { version = "=2.0.1", optional = true }` and `datagen = ["dep:viriformat"]`, matching bullet's pin ([utils/Cargo.toml:17](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils/Cargo.toml#L17)). ADR-0005's "Any new runtime dependency requires amending this ADR" then applies, even behind a feature.
- **Search:** add `soft_nodes: Option<u64>` to `Limits` ([worker.rs:109-116](../../src/search/worker.rs)) and check it next to `soft_time_is_up()` at [worker.rs:230](../../src/search/worker.rs). Outside datagen it is `None`, so this is **No functional change** for UCI and Bench.
- **Entry point:** `mudskipper datagen <threads> <games-or-0> <out-dir>` in `main.rs` behind `#[cfg(feature = "datagen")]` ([main.rs:14-24](../../src/main.rs)). Write one `.vf` file per thread, as Viridithas and Stormphrax do.
- **Writer must-haves:**
  - A **fullmove ≥ 1** (section 2).
  - Unmoved castling rooks as type 6.
  - White-relative `i16` scores.
  - The result byte filled in after the game ends.
  - A unit test that round-trips the README's example bytes ([README.md:43-60](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md#L43-L60)).

### Step 2. Datagen settings (gen 1, PeSTO)

| Setting | Value | Source / reason |
|---|---|---|
| Openings | startpos + 8 or 9 uniformly random legal moves; skip a game if the random moves end it | akimbo v0.6.0 (same architecture's first net), Stormphrax |
| Opening check | search 50k nodes, discard if \|score\| > 1000 cp | Viridithas `datagen.rs:517-531` |
| Per-move limit | soft 10,000 nodes, hard 80,000 (8 × soft) | section 2 estimate; Viridithas's 8× ratio |
| Win adjudication | \|score\| ≥ 2500 for 4 consecutive plies, or a mate score | Viridithas |
| Draw adjudication | \|score\| ≤ 4 for 12 consecutive plies; plus 50-move, threefold, insufficient material, stalemate | Viridithas; mudskipper `draw.rs`, `key_history.rs` |
| What to store | every move and score (filter at splat time) | Viridithas, Stormphrax viriformat |
| Workers | 32 threads (one game each; drop to 28–30 if using the PC) | all three references |
| Per worker | fresh `Worker` + `TranspositionTable::new(8)` cleared per game | akimbo 8 MB; mudskipper history persistence |

### Step 3. Targets

- **Pilot:** 15–20M usable positions, about 25–30M stored. Roughly **2 h** (estimate, section 2).
- **First real network:** **100M usable** positions, about 150M stored. Roughly **13 h** (estimate). Disk: about 0.65 GB of viriformat.
- Replace the 50 Mnps and 65% assumptions with the pilot's measured figures before planning the long run.

### Step 4. Safe shuffling workflow (route B)

```text
bullet-utils viribinpack interleave out/0.vf out/1.vf … -o gen1.vf          # streaming, O(1) RAM
bullet-utils viribinpack splat gen1.vf gen1.bin filter.toml               # filter.toml MUST exist
#   filter.toml:  min_ply = 16   (other fields default: min_pieces 4, no noisy best move, not in check)
#   check: size(gen1.bin) / 32 = kept positions; it must be well below the "Positions" it prints
bullet-utils validate --input gen1.bin
bullet-utils shuffle -i gen1.bin -o gen1.shuffled.bin -m 8192             # ≤ 8 GiB RAM
```

- **Peak RAM:** about 3.2 GB for 100M positions (in-memory path), about 2.4–8 GiB for 300M (chunked path).
- **Free disk needed:** up to 2× the `.bin` size.
- **Later generations:** shuffle each generation's `.bin` separately, then `bullet-utils interleave a.shuffled.bin b.shuffled.bin -o all.bin`.
- **Route A instead:** skip `splat`/`shuffle` and use `ViriBinpackLoader` with `buffer_size_mb: 4096` (a 2 GiB, ≈ 67M-position shuffle window). That is a valid alternative once data outgrows comfortable disk space, but its shuffle is only windowed.

### Step 5. Train

- **Setup:** a checkout of bullet at `5d70177`, CUDA Toolkit installed, `CUDA_PATH` set, and the section 4 config copied into a new registered example.
- **Pilot:** run `cargo r -r --example mudskipper_gen1 --features cuda` twice, once with `wdl_proportion` 0.75 and once with 0.4, using `batches_per_superbatch 1024, superbatches 20`. Keep the winner of the SPRT between the two.
- **First real network:** the full 40 × 6104 schedule at 100M.
- **Output:** `checkpoints/gen1-40/quantised.bin`, which must be 394,816 bytes. Publish it to the networks repo (ADR-0004) and build with `EVALFILE=…`.

### Step 6. Validate

Run the section 5 checks in order:

1. `validate` and the W/D/L split.
2. A falling `log.txt` loss.
3. `trainer.eval` × 400 against engine output.
4. Startpos sign, colour-mirror equality and incremental-versus-refresh equality, all in `cargo test`.
5. A calibration bucket table.
6. A new Bench signature.

### Step 7. SPRT

1. **Pilot versus PeSTO master.** Confirms the pipeline and gives a lower bound.
2. **gen1 (100M) versus the pilot**, or versus PeSTO if the pilot was not merged.
3. **Re-tune the eval-based margins** with their own SPRTs.
4. **Gen 2:** datagen with the gen-1 engine, appended to the gen-1 data; SPRT gen2 versus gen1.

## 7. Sources

**bullet**: [`jw1912/bullet@5d70177`](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c)
- [docs/2-getting-started.md](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/2-getting-started.md), [docs/3-data.md](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/3-data.md), [docs/4-saved-networks.md](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md)
- [examples/simple.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs), [examples/progression/1_simple.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs), [examples/advanced/main.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/main.rs), [examples/advanced/filter.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/advanced/filter.rs)
- [crates/utils/](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/utils): `Cargo.toml`, `src/shuffle.rs`, `src/interleave.rs`, `src/convert.rs`, `src/validate.rs`, `src/viribinpack/{splat,interleave,count}.rs`
- [crates/bullet_lib/](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib): `Cargo.toml`, `src/nn.rs`, `src/value.rs`, `src/value/save.rs`, `src/value/loader/{viribinpack,direct}.rs`, `src/trainer/{schedule,settings}.rs`, `src/trainer/schedule/{lr,wdl}.rs`, `src/game/inputs/chess768.rs`
- [crates/trainer/src/reader/fixed_size.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/reader/fixed_size.rs), [crates/trainer/src/run.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/run.rs), [crates/gpu/build.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/gpu/build.rs)

**bulletformat**: [`jw1912/bulletformat@eddd9a2`](https://github.com/jw1912/bulletformat/tree/eddd9a2d77dbc1391d68f23ba524657e91bd9a73) (v1.8.0)
- [src/chess.rs](https://github.com/jw1912/bulletformat/blob/eddd9a2d77dbc1391d68f23ba524657e91bd9a73/src/chess.rs)

**viriformat**: [`cosmobobak/viriformat@0238b31`](https://github.com/cosmobobak/viriformat/tree/0238b315ea620acbf3b4d0fd8b61261efa59b0ae) (v3.0.0), and the [2.0.1 crate](https://docs.rs/crate/viriformat/2.0.1/source/) (`.cargo_vcs_info.json` git `5967bba`, `.crate` sha256 `3e889c0f…`) that bullet pins
- [README.md](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/README.md) (spec)
- [src/dataformat.rs](https://github.com/cosmobobak/viriformat/blob/0238b315ea620acbf3b4d0fd8b61261efa59b0ae/src/dataformat.rs); 2.0.1 [src/dataformat.rs](https://docs.rs/crate/viriformat/2.0.1/source/src/dataformat.rs), [src/chess/board/mod.rs](https://docs.rs/crate/viriformat/2.0.1/source/src/chess/board/mod.rs), [Cargo.toml](https://docs.rs/crate/viriformat/2.0.1/source/Cargo.toml)

**akimbo**: [`jw1912/akimbo@f7dd767`](https://github.com/jw1912/akimbo/tree/f7dd7677d6ab9a4a76e3769731084894a7f7ae09) (HEAD, README only); [v1.0.0 `3bfea08`](https://github.com/jw1912/akimbo/tree/3bfea08c8c56c462a28cb1f3b76a442938e8085f); [v0.6.0 `ca0345e`](https://github.com/jw1912/akimbo/tree/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0)
- v1.0.0: [Cargo.toml](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/Cargo.toml), [Makefile](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/Makefile), [src/datagen.rs](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/datagen.rs), [src/main.rs](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/main.rs), [src/search.rs](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/search.rs), [src/position.rs](https://github.com/jw1912/akimbo/blob/3bfea08c8c56c462a28cb1f3b76a442938e8085f/src/position.rs)
- v0.6.0: [README.md](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/README.md), [datagen/src/main.rs](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/main.rs), [datagen/src/thread.rs](https://github.com/jw1912/akimbo/blob/ca0345ee7fc4b4d327510a3d64a19143a8ead8f0/datagen/src/thread.rs)

**Viridithas**: [`cosmobobak/viridithas@4609ff2`](https://github.com/cosmobobak/viridithas/tree/4609ff2a898160f1576fe83c0529a949148541c0)
- [Cargo.toml](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml), [src/datagen.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/datagen.rs), [src/timemgmt.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/timemgmt.rs), [README.md](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/README.md), [notes/networkhistory.txt](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/notes/networkhistory.txt)

**Stormphrax**: [`Ciekce/Stormphrax@d1468d9`](https://github.com/Ciekce/Stormphrax/tree/d1468d99e3d3733100d20d682ac627998cce94b2)
- [src/datagen/datagen.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/datagen.cpp), [src/datagen/viriformat.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/viriformat.cpp), [src/datagen/marlinformat.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/datagen/marlinformat.cpp), [src/main.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/main.cpp), [src/search.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp), [src/limit.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/limit.cpp), [CMakeLists.txt](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/CMakeLists.txt), [README.md](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/README.md), [network.txt](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/network.txt)

**Carp**: [`dede1751/carp@7d5065b`](https://github.com/dede1751/carp/tree/7d5065bc26dd16002e6581b25abb3decf62ed86c) (citations reused from nnue-inference.md, not re-read)
- [chess/src/nnue/nnue_history.txt](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt)

**mudskipper** at `f95fa5d`
- [src/search/worker.rs](../../src/search/worker.rs), [src/search.rs](../../src/search.rs), [src/search/tt.rs](../../src/search/tt.rs), [src/search/score.rs](../../src/search/score.rs)
- [src/uci.rs](../../src/uci.rs), [src/uci/options.rs](../../src/uci/options.rs), [src/main.rs](../../src/main.rs), [src/tools/bench.rs](../../src/tools/bench.rs)
- [src/eval.rs](../../src/eval.rs), [src/eval/pesto.rs](../../src/eval/pesto.rs)
- [src/chess/board.rs](../../src/chess/board.rs), [src/chess/board/draw.rs](../../src/chess/board/draw.rs), [src/chess/board/movegen.rs](../../src/chess/board/movegen.rs), [src/chess/moves.rs](../../src/chess/moves.rs), [src/chess/key_history.rs](../../src/chess/key_history.rs), [src/chess/game.rs](../../src/chess/game.rs)
- ADRs [0001](../adr/0001-chess960-native-castling.md), [0004](../adr/0004-network-outside-repo.md), [0005](../adr/0005-dependencies-and-unsafe.md), [0006](../adr/0006-pesto-until-nnue.md)
- Measured: `mudskipper bench` (depth 8): 726,262 nodes, 2,999,843 nps, release build, one thread of an i9-9900K VM (4 vCPUs visible)
