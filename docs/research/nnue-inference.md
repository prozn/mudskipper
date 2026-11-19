# NNUE inference for a first network

Research for adding NNUE inference to mudskipper, replacing PeSTO (ADR-0006). Written 2026-11-19 against mudskipper at `f95fa5d`.

Each claim links to source at a pinned commit, or to crate or Cargo docs. Viridithas is the main style reference. Its current network is far beyond a first network (threat and pawn-pair inputs, king buckets, three layers), so its **v9.0.0** tag stands in for a simple one. That release ran a `(768 → 512)x2 → 1` SCReLU network with QA 255, QB 64 and scale 400, and Carp's notes say Carp's inference copies it ([nnue_notes.txt:1-5](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_notes.txt#L1-L5)). Section 7 says what a first version needs, and section 8 lists what waits for v2.

Pinned sources (see the [Sources](#9-sources) list):

| Source | Commit | What it is |
|---|---|---|
| bullet | `5d70177` (2026-11-05) | The trainer: docs, examples, save format |
| Viridithas (HEAD) | `4609ff2` (2026-11-17) | Rust engine, current architecture |
| Viridithas v9.0.0 | `5d3ef87` (2023-05-08) | Rust engine, last simple `768 → N` perspective net |
| akimbo | `f7dd767` (2025-02-07) | Rust engine by bullet's author |
| Carp | `7d5065b` (2025-01-18) | Rust engine, `(768 → 1024)x2 → 1` |

**Carp's location.** The brief named `Sp00ph/Carp`, which does not exist on GitHub ("Repository not found"). Carp lives at [`dede1751/carp`](https://github.com/dede1751/carp/tree/7d5065bc26dd16002e6581b25abb3decf62ed86c), by Andrea Sgobbi ([chess/Cargo.toml](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/Cargo.toml)), and that is the repository inspected here.

Terms: **accumulator** means the hidden layer's pre-activation vector, `a = Hx + b`, kept per perspective ([bullet 1-basics.md:24, 78](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L24)). **Perspective** means the side whose view a set of features takes. **stm** and **ntm** mean side to move and not side to move. N is the hidden size per perspective.

## 1. Summary

- **Architecture.** Every bullet example is a perspective network `(768 → N)x2 → 1`. bullet's docs call white-relative, non-perspective inputs "much worse". Every simple example uses SCReLU. Carp measured SCReLU at +4.7 Elo over CReLU at N = 512. bullet's examples use N = 128. Carp's first network used N = 256 on 100M positions, and Carp could not use 384 or 512 until it had more data. Viridithas v9 shipped N = 512.
- **Accumulator.** No source keeps the accumulator inside a copied board. Carp and Viridithas v9 update a separate per-ply stack eagerly as each move is made. Current Viridithas records each move's changes and applies them lazily, only when a node is actually evaluated. akimbo keeps no per-ply accumulator at all: `Position` is `Copy` with no accumulator, and `eval` diffs the bitboards against a per-thread cache. Putting N = 256 inside `Board` would take it from 216 bytes (measured) to 1280 bytes, copied on every `make`.
- **Quantisation.** Every source quantises with QA = 255 and QB = 64, using `i16` throughout. With SCReLU the output is `(Σ clamp(a,0,QA)² · w / QA + bias) · SCALE / (QA · QB)`, dividing by QA before the bias is added. CReLU drops the `/ QA`. SCALE is 400 everywhere except current Viridithas (240). The network's output is in whatever units the training scores were in, so SCALE does not need to match anything in PeSTO, but it does fix how centipawns map to win probability.
- **SIMD.** Carp (N = 1024) and Viridithas v9 (N = 512) contain no `std::arch` in their inference at all. They rely on auto-vectorisation with `-C target-cpu`. akimbo selects one hand-written AVX2 SCReLU loop or a scalar fallback with compile-time `#[cfg(target_feature = "avx2")]`. Current Viridithas also selects at compile time, between AVX-512, AVX2 and NEON. No source uses runtime CPU detection. akimbo multiplies by the weight before squaring (`madd(v, mullo(v, w))`), which is exact because `255 × 127` still fits in `i16`.
- **Embedding.** bullet writes the tensors in save-format order as little-endian values, each matrix column-major, with no per-tensor padding, then pads the whole file to a multiple of 64 bytes with the repeated bytes `bullet`. akimbo, Carp and Viridithas v9 all `transmute` `include_bytes!` into a `#[repr(C)]` struct of `#[repr(C, align(64))]` arrays. None of them uses bytemuck. Viridithas compresses its network with zstd and decompresses it at startup with `ruzstd` (pure Rust), or with the `zstd` crate for release builds.
- **Recommendation.** Use `(768 → 256)x2 → 1` with SCReLU, QA 255, QB 64 and SCALE 400. Keep the accumulators in a stack of `MAX_PLY + 1` entries owned by `Worker`, not in `Board`, and fill each child's entry eagerly from its parent's when the search enters it. Start with scalar code and no `unsafe`, and parse the embedded bytes with `i16::from_le_bytes` into heap-allocated, 64-byte-aligned rows. That needs **no new dependency and no ADR-0005 amendment**. Section 7 has the details.

## 2. Question 1: first architecture

### Perspective vs non-perspective

bullet's docs describe both. `768 → N → 1` uses white-relative inputs ([1-basics.md:7-11, 17](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L7-L17)). The perspective net `768 → Nx2 → 1` computes `y = O₁ρ(a) + O₂ρ(â) + c`, with one accumulator from the side to move's view and one from the other side's ([1-basics.md:72-91](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L72-L91)). The docs give the reason:

> Using white relative inputs as in the above example is much worse than using side-to-move relative inputs. This is where you flip the board so the inputs are always from the "perspective" of the side-to-move, which encodes "tempo" in the most natural way. ([1-basics.md:64](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L64))

Side-to-move-relative inputs already need both accumulators kept up to date ([1-basics.md:66](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L66)), so feeding both into the output layer costs nothing extra to maintain.

bullet's examples no longer contain a non-perspective network. Every example calls `.dual_perspective()` and concatenates stm and ntm activations ([simple.rs:27, 46-56](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L46-L56); [progression/1_simple.rs:22, 32-42](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L22-L42)). Every engine inspected is perspective: akimbo ([network.rs:35-39](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L35-L39)), Carp ([nnue/mod.rs:1-2, 199-216](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L199-L216)) and Viridithas v9 ([network.rs:439-448](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L439-L448)).

**Feature index.** bullet's `Chess768` maps a piece to `stm = [0, 384][colour] + 64·piece + sq` and `ntm = [384, 0][colour] + 64·piece + (sq ^ 56)` ([chess768.rs:20-30](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/game/inputs/chess768.rs#L20-L30)). In engine terms (Viridithas v9, [network.rs:238-251](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L238-L251); Carp, [nnue/mod.rs:220-230](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L220-L230)) that becomes:

```text
white_index = colour·384       + piece_type·64 + square
black_index = (1 − colour)·384 + piece_type·64 + (square ^ 56)
```

Here pawn = 0 … king = 5 and a1 = 0, which are mudskipper's own numberings ([piece.rs](../../src/chess/piece.rs), [square.rs](../../src/chess/square.rs)). *Unverified:* whether bullet's `ChessBoard` (from the external `bulletformat` crate, not inspected) stores positions stm-relative with pawn = 0. The engines' indexing is consistent with that.

### SCReLU vs CReLU

| Source | Activation | Evidence |
|---|---|---|
| bullet `simple.rs` and progression 1–3 | SCReLU | [simple.rs:52-53](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L52-L53), [1_simple.rs:38-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L38-L39) |
| bullet progression 4 (multi-layer) | CReLU on the feature transformer, SCReLU later | [4_multi_layer.rs:79-85](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/4_multi_layer.rs#L79-L85) |
| akimbo | SCReLU | [network.rs:152-154, 184-186](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L152-L154) |
| Carp | SCReLU, after starting on CReLU | [nnue/mod.rs:232-237](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L232-L237); CReLU write-up [nnue_notes.txt:102-115](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_notes.txt#L102-L115) |
| Viridithas v9 | SCReLU in use; CReLU kept as dead code | [network.rs:538-582](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L538-L582) |

The only measurement found: Carp's switch to "SquaredReLu" at N = 512 scored **+4.71 ± 3.73** against the same setup with CReLU ([nnue_history.txt:54-55](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L54-L55)). SCReLU costs one extra multiply per hidden unit and an extra `/ QA` (section 4). With CReLU on the feature transformer, bullet's only examples are its multi-layer ones.

### Hidden size N

| Source | N | Note |
|---|---|---|
| bullet `simple.rs`, `1_simple.rs` | 128 | [simple.rs:19](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L19), [1_simple.rs:14](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L14) |
| Carp's first net (carp0000) | 256 | 100M depth-8 self-play positions, **+518 vs its HCE** ([nnue_history.txt:4-5](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L4-L5)) |
| Viridithas v9 | 512 | [network.rs:29](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L29) |
| Carp, akimbo, current Viridithas | 1024 | [Carp nnue/mod.rs:21](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L21), [akimbo network.rs:3](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L3), [Viridithas network.rs:60](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L60) |

Carp's network log shows N tracking the amount of data:

- 384 on the same 100M positions lost (carp0001, [:9-10](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L9-L10)), then gained +22.5 once it had 230M ([:19-20](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L19-L20)).
- 512 lost at 230M, "Not enough data for 512 nodes yeat" ([:29-30](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L29-L30)), then gained at 670M ([:49-50](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L49-L50)).
- 768 needed 1.68B ([:74-85](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L74-L85)), and 1024 needed 3.26B ([:99-100](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt#L99-L100)).

bullet's docs advise widening the first layer rather than adding layers, "up to at least 1024" ([1-basics.md:120-124](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L120-L124)), and starting from plain 768 inputs ([:114-118](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L114-L118)).

## 3. Question 2: accumulator lifecycle under copy-make

mudskipper copies the board on every move (ADR-0003). [`Board::make`](../../src/chess/board/make.rs) clones `self`, edits the clone and returns it ([make.rs:19-75](../../src/chess/board/make.rs)). `pass` does the same for null moves ([make.rs:90-106](../../src/chess/board/make.rs)). The search makes a child after the legality check, then calls `Worker::enter` with the parent board ([worker.rs:374-385, 633-634](../../src/search/worker.rs)). `Worker` already keeps a per-ply `stack: [Frame; MAX_PLY + 1]` with `MAX_PLY = 128` ([worker.rs:154-156](../../src/search/worker.rs), [search.rs:25](../../src/search.rs)). Evaluation is `eval::evaluate(board)` with only the board in hand ([eval.rs:19-21](../../src/eval.rs)). It is called at the ply limit ([worker.rs:343, 587](../../src/search/worker.rs)) and for the static evaluation, which comes **from the transposition table when the entry holds one** ([worker.rs:836-841](../../src/search/worker.rs)) and is skipped at nodes in check ([worker.rs:687-697](../../src/search/worker.rs)).

### What the sources do

| Engine | Board model | Where the accumulator lives | When it is updated |
|---|---|---|---|
| Carp | Copy-make (`make_move` returns a new `Board`) | Separate `NNUEState { accumulator_stack: [Accumulator; MAX_DEPTH + 1], current_acc }`, boxed ([nnue/mod.rs:123-128, 137-154](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L123-L154)) | **Eagerly** in `make_move_nnue`: `push()` copies the top entry, then applies the move's updates in place ([make_move.rs:74-116](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/movegen/make_move.rs#L74-L116); [nnue/mod.rs:170-192](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L170-L192)) |
| Viridithas v9 | Make/unmake | Separate stack `accumulators: [Accumulator<LAYER_1_SIZE>; ACC_STACK_SIZE]` ([network.rs:233-235](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L233-L235)) | **Eagerly**: `push_acc` copies, then `efficiently_update_*` apply in place ([network.rs:284-378](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L284-L378); called from [board.rs:1049-1100](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/board.rs#L1049-L1100)) |
| Viridithas HEAD | Make/unmake | Separate stacks plus per-ply `UpdateBuffer`, `moves` and `*_correct` dirty flags ([network.rs:1391-1412](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L1391-L1412)) | **Lazily**: `make_move_nnue` only records the changes and marks the child not correct ([board/mod.rs:1081-1106](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/chess/board/mod.rs#L1081-L1106)). `evaluate` calls `force`, which walks back to the last correct entry and materialises forward, source → target ([evaluation.rs:143-147](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/evaluation.rs#L143-L147); [network.rs:1513-1574](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L1513-L1574)) |
| akimbo | Copy-make (`let mut new = *pos; new.make(...)`, [search.rs:234-235](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/search.rs#L234-L235)) | **Nowhere per ply.** `Position` is `Copy` and holds only bitboards and keys ([position.rs:10-19](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/position.rs#L10-L19)). One `EvalTable` per thread caches an accumulator pair and the bitboards it was built from, per king-bucket pair ([network.rs:111-134](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L111-L134); [thread.rs:30](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/thread.rs#L30)) | **At eval only**: `eval` diffs the position's bitboards against the cached ones, applies the adds and subs, and stores the new bitboards ([position.rs:180-200, 217-250](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/position.rs#L180-L200)) |

No inspected engine stores the accumulator inside the board it copies.

### Accumulator in `Board` vs a separate stack, for mudskipper

`size_of::<Board>()` is **216 bytes, align 8** at `f95fa5d`, measured with a scratch crate depending on mudskipper. The fields are the mailbox `[Option<Piece>; 64]` (128 B), eight bitboards (64 B) and the scalars ([board.rs:25-36](../../src/chess/board.rs)). One accumulator pair is `2 perspectives × N × 2 bytes`:

| N | Accumulator pair | `Board` with it inside (align 64) | Separate stack of `MAX_PLY + 1` = 129 pairs |
|---|---|---|---|
| 128 | 512 B | 768 B (3.6× today) | 64.5 KiB |
| 256 | 1 KiB | 1280 B (5.9× today) | 129 KiB |
| 512 | 2 KiB | 2304 B (10.7× today) | 258 KiB |

The middle column is my arithmetic: 216 B plus the pair, rounded up to the 64-byte alignment the pair needs.

- **Inside `Board`.** Every `make` and `pass` copies the whole pair, including on the many children that are never evaluated. It also bloats every other `Board` copy (`Game`, the UCI position) and adds alignment to a type in `chess`, which "never depends on the rest of the engine" ([chess.rs:1-3](../../src/chess.rs)). The network would then become a dependency of `chess`. The upside is that the network needs no plumbing.
- **Separate stack in `Worker`.** This matches Carp and both Viridithas versions, and mudskipper already has the hook: `Worker::enter(board, ply, reached)` runs once per child, with the parent board, before the child is searched ([worker.rs:734-741](../../src/search/worker.rs)). Passing it the move as well gives everything needed to compute the change. `Board::make` stays untouched and `chess` stays independent of the network. A 129 KiB stack should be heap-allocated. Carp and Viridithas use `alloc_zeroed` with `unsafe` for this ([Carp nnue/mod.rs:137-145](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L137-L145); [Viridithas v9 network.rs:255-276](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L255-L276)). A `vec![…; MAX_PLY + 1].into_boxed_slice()` does it without `unsafe`.

**Fused copy and update.** Carp and Viridithas v9 first copy the parent entry into the child, then update the child in place. Current Viridithas reads the source accumulator and writes the target in one pass ("materialise … from", [network.rs:1542-1567](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L1542-L1567); the in-place loop shape is at [accumulator.rs:56-104](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/accumulator.rs#L56-L104)). With a stack indexed by ply, `child = parent − subs + adds` reads the parent and writes the child once, with no separate copy.

### Eager vs lazy updates

- **Eager**, in `enter`: every searched child pays one update of at most 2 adds and 2 subs per perspective. That covers children that then return straight away on a draw ([worker.rs:339-341](../../src/search/worker.rs)), a TT cutoff ([:346-349](../../src/search/worker.rs)), a TT-stored static evaluation ([:836-841](../../src/search/worker.rs)) or check, where no static evaluation is made ([:694](../../src/search/worker.rs)).
- **Lazy**, Viridithas HEAD's scheme: `enter` records the change and clears the child's "correct" flag. Evaluation walks back to the nearest correct ancestor and applies the recorded changes forward ([network.rs:1513-1574](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L1513-L1574)). Unevaluated nodes then cost nothing beyond storing a few indices. The price is a correct flag and a change record per ply, plus the walk.

The two give **identical evaluations**, so moving from eager to lazy leaves the Bench signature unchanged. That makes it a **No functional change** speedup, checked by NPS, which needs no SPRT. So the simpler eager version can land first without blocking the lazy one.

akimbo's eval-time diff is a third option. It changes neither `make` nor `Worker`, but each evaluation diffs 12 piece bitboards against the cache, and the diff grows when consecutive evaluations sit far apart in the tree. *Unverified:* its cost at mudskipper's node rates without king buckets. akimbo pairs it with buckets.

### King buckets, Finny tables, horizontal mirroring: postpone

All three come together in the sources, and all three are input-side refinements:

- akimbo's `BUCKETS` table and `ksq ^= 7` mirroring ([network.rs:20-59](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L20-L59)).
- Viridithas's `HALF_BUCKET_MAP` and `BucketAccumulatorCache`, the Finny table, "BUCKETS * 2 … because we use a horizontally-mirrored architecture" ([network.rs:102-133, 1322-1330](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L1322-L1330)).
- A king move that changes bucket forces a refresh ([network.rs:1480-1486](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L1480-L1486)).

bullet's progression puts input buckets third, after output buckets ([examples/progression/](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression)), and warns that a beginner "won't have enough data" for king-relative features "at first" ([1-basics.md:114-118](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L114-L118)). A Finny table exists only to make bucket refreshes cheap, and mirroring only to halve the bucket count. Without buckets neither has a job. **All three should wait.**

## 4. Question 3: quantisation and scaling

### QA and QB

| Source | QA | QB | Bias of output | Activation |
|---|---|---|---|---|
| bullet `simple.rs` | 255 | 64 | QA·QB | SCReLU ([simple.rs:20-22, 34-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L20-L39)) |
| bullet `1_simple.rs` | 255 | 64 | 255·64 | SCReLU ([1_simple.rs:25-30](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L25-L30)) |
| akimbo | 255 | 64 | QA·QB | SCReLU ([network.rs:4-7](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L4-L7)) |
| Carp | 255 | 64 | 255·64 | SCReLU now ([nnue/mod.rs:27-29](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L27-L29)), CReLU earlier with the same constants ([convert_json.py:9-11](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/convert_json.py#L9-L11)) |
| Viridithas v9 | 255 | 64 | 255·64 | SCReLU; CReLU code uses the same ([network.rs:21-33](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L21-L33)) |

No source changes QA or QB between CReLU and SCReLU. The feature weights and biases are quantised by QA, the output weights by QB and the output bias by QA·QB ([simple.rs:35-38](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L35-L38)). By default `.quantise` truncates, and `.round()` makes it round ([4-saved-networks.md:53-59](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L53-L59); [weights.rs:290-292, 311-320](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L290-L320)). The examples round. A value that does not fit the integer type makes saving the quantised network fail ([weights.rs:315-317](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L315-L317); [4-saved-networks.md:10](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L10)).

**Why 255 and 64 fit in `i16`.** bullet's AdamW clips every weight to [−1.98, 1.98] by default ([adam.rs:27-29](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/optimiser/adam.rs#L27-L29); comment at [simple.rs:28-29](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L28-L29)). From that clip, by my arithmetic:

- A feature weight is at most `1.98 × 255 ≈ 505`. With at most 32 active features plus the bias, `|a| ≤ 33 × 505 = 16665`, under `i16::MAX` (32767).
- An output weight is at most `1.98 × 64 ≈ 127`, so `clamp(a, 0, 255) × w ≤ 255 × 127 = 32385` also fits in `i16`. That is what makes the multiply-before-square trick in section 5 exact.
- The `i32` sum is **not** guaranteed: SCReLU's worst case is `2N × 255² × 127`, which passes `i32::MAX` once N ≥ 131. No source guards against this. They rely on real activations and weights never all saturating together.

### Dequantising to centipawns

bullet's reference inference, quoted with its order of operations ([simple.rs:153-180](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L153-L180)):

```rust
output += screlu(input) * i32::from(weight);   // over stm then ntm; scale QA·QA·QB
output /= i32::from(QA);                        // "Reduce quantization from QA * QA * QB to QA * QB."
output += i32::from(self.output_bias);          // bias is at QA·QB
output *= SCALE;                                // "Apply eval scale."
output /= i32::from(QA) * i32::from(QB);        // "Remove quantisation altogether."
```

That is:

```text
SCReLU:  eval = (Σ clamp(a,0,QA)² · w  / QA  +  b) · SCALE / (QA · QB)
CReLU:   eval = (Σ clamp(a,0,QA)  · w        +  b) · SCALE / (QA · QB)
```

with Rust integer division, which truncates toward zero, at each `/`. The same order appears in:

- akimbo: `(sum / QA + i32::from(NNUE.output_bias)) * SCALE / QAB` ([network.rs:35-39](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L35-L39)).
- Carp: `((out / QA + MODEL.output_bias as i32) * SCALE / QAB)` ([nnue/mod.rs:215](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L215)).
- Viridithas v9: `screlu_flatten` returns `sum / QA` ([network.rs:581](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L581)), then `(output + bias) * SCALE / QAB` ([:447](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L447)).

For CReLU, the flatten returns the raw sum ([v9 network.rs:546-558](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L546-L558)), and Carp's CReLU-era notes give `out * SCALE / QAB` with the bias already in `out` ([nnue_notes.txt:102-115](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_notes.txt#L102-L115)). Engines keep this integer path so that the evaluation, and so the Bench signature, is the same on every platform, as PeSTO's is now ([eval.rs:15-17](../../src/eval.rs)).

Current Viridithas also clamps the network output away from tablebase-win scores ([evaluation.rs:120-129](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/evaluation.rs#L120-L129)). mudskipper would need the same against its mate band (`MATE_BOUND = 32000 − 128`, [score.rs:10-13](../../src/search/score.rs)). The transposition table stores the static evaluation clamped to `i16` ([tt.rs:137-138](../../src/search/tt.rs)), which any clamp below the mate band already satisfies.

### What `scale` means, and how it relates to PeSTO

In training, bullet builds each target as `blend · result + (1 − blend) · sigmoid(score / eval_scale)` ([value.rs:64-67, 109, 113-115](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L64-L115); the comment in [simple.rs:42](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L42)). The loss fits `sigmoid(output)` to that target ([simple.rs:44](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L44)). So the float output is "score / SCALE", and multiplying by SCALE at inference returns it to the units of the scores in the training data. The examples use `eval_scale: 400.0` and the matching inference constant `SCALE = 400` ([simple.rs:20, 60](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L60); [1_simple.rs:46](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/1_simple.rs#L46)). akimbo, Carp and Viridithas v9 all use 400. Current Viridithas uses 240 ([network.rs:68-71](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L68-L71)); *unverified:* why.

For mudskipper:

1. PeSTO has no scale constant of its own. It returns centipawns with pawn ≈ 82 in the middlegame and 94 in the endgame ([pesto.rs:9-13](../../src/eval/pesto.rs)). If the first network trains on mudskipper's own search scores (ADR-0006), its output comes out in those same PeSTO-based centipawns. That holds for any SCALE, as long as training and inference use the same value.
2. SCALE also sets how centipawns map to win probability, through the sigmoid. With a WDL blend above 0, the network is pulled towards whatever centipawn scale makes `sigmoid(cp / SCALE)` match game results. If PeSTO's centipawns imply a different win probability for the same number, the network's evaluations will drift in magnitude away from PeSTO's. Then the centipawn margins tuned under PeSTO drift too: reverse futility, futility and the aspiration window ([worker.rs:79-100](../../src/search/worker.rs)). *Suggestion, not from a source:* before training, fit the sigmoid constant to mudskipper's data (score versus result), and use SCALE = 400 unless the fit is far from it. Either way, plan to re-check the eval-based margins once the network lands.

## 5. Question 4: SIMD vs safe crates vs auto-vectorisation

### What each engine does

| Engine | Inference code | Gating | Scalar fallback | Runtime detection |
|---|---|---|---|---|
| Carp (N = 1024) | Plain iterator loops ([nnue/mod.rs:84-120, 199-216](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L199-L216)); no `std::arch` outside a TT prefetch | `-C target-cpu=…` per build ([makefile](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/makefile)) | It *is* scalar | No |
| Viridithas v9 (N = 512) | Plain loops ([network.rs:500-582](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L500-L582)); `std::arch` only for a TT prefetch | — | It *is* scalar | No |
| akimbo | One AVX2 `flatten`, plus a register-blocked scalar accumulator update that relies on auto-vectorisation ([network.rs:69-102](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L69-L102)) | `#[cfg(target_feature = "avx2")]` vs `#[cfg(not(...))]` ([network.rs:136-191](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L136-L191)); release builds a baseline and a `+avx2` binary ([Makefile](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/Makefile)) | Yes ([network.rs:147-166](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L147-L166)) | No |
| Viridithas HEAD | Wrapped-register SIMD layer (`VecI16` etc.) over AVX-512, AVX2 and NEON ([simd.rs:37-55](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/simd.rs#L37-L55)) | Compile time: `avx512` if `avx512f`, `neon` if `neon`, **otherwise `avx2`** ([simd.rs:57, 481, 840, 1262-1269](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/simd.rs#L1262-L1269)) | None found in `simd.rs` | No |

None of the four uses `is_x86_feature_detected!` (searched all four trees). All select code with `cfg`, `-C target-cpu`, or both.

### The multiply-before-square trick

akimbo's AVX2 SCReLU ([network.rs:173-191](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L173-L191)):

```rust
let mut v = load_i16s(acc, i * CHUNK);
v = _mm256_min_epi16(_mm256_max_epi16(v, min), max);          // clamp to [0, QA]
let w = load_i16s(weights, i * CHUNK);
let product = _mm256_madd_epi16(v, _mm256_mullo_epi16(v, w));   // v · (v·w), pairwise-summed to i32
sum = _mm256_add_epi32(sum, product);
```

`mullo` keeps the low 16 bits of `v·w`. That is exact because `|v·w| ≤ 255 × 127 = 32385` (section 4). Then `madd` widens to `i32` while multiplying by `v` again and adding adjacent pairs. Squaring first would give `v² ≤ 65025`, which overflows `i16`. So the order "clamp, times weight, times clamped value" is what lets the whole SCReLU stay in 16-bit lanes until the final widening multiply. akimbo's scalar fallback skips the trick and widens to `i32` up front: `i32::from(x.clamp(0, QA as i16)).pow(2)` ([network.rs:152-154](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L152-L154)). bullet's reference does the same ([simple.rs:126-129](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L126-L129)).

*Unverified:* whether rustc auto-vectorises the scalar `i32` form into something as tight. Carp and Viridithas v9 shipped that scalar form at N = 1024 and N = 512. Writing the scalar loop as `(v * w) as i32 * v` with `v, w: i16` lets the compiler see the same trick. That is safe Rust, and it is exact under the bound above.

### Safe SIMD crates

- **`wide`** 1.7.1 offers `i16x16` and other fixed-width types. It depends on `bytemuck` and `safe_arch` ([docs.rs/wide](https://docs.rs/wide/1.7.1/wide/)). `safe_arch` "works purely via `#[cfg()]` and compile time CPU feature declaration", so its functions "don't exist at all when the appropriate CPU target features aren't enabled", and runtime detection is not supported ([docs.rs/safe_arch](https://docs.rs/safe_arch/1.2.0/safe_arch/)). That is the same compile-time model as akimbo, and it adds two runtime crates.
- **`pulp`** 0.22.3 is "a safe abstraction over SIMD instructions" that dispatches "based on the features detected at runtime" via `Arch::new()` and `dispatch` ([docs.rs/pulp](https://docs.rs/pulp/0.22.3/pulp/)). It pulls in nine normal dependencies, including `bytemuck`, `raw-cpuid`, `num-complex` and `libm`. *Unverified:* how much `unsafe` it uses internally; its docs don't say.
- No inspected engine uses either crate. Viridithas, the most SIMD-heavy, writes its own wrapper over `std::arch` ([simd.rs:37-55](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/simd.rs#L37-L55)).

### How this fits mudskipper's build

mudskipper's default `make` builds a **portable** binary with no `target-cpu`, and SPRT builds pass `CPU=native` ([Makefile:1-24](../../Makefile)). Under compile-time gating, the default build gets the scalar path and SPRT builds get AVX2. Engine-vs-engine comparisons stay fair because both sides of an SPRT build the same way. Runtime detection would be the way to make the portable binary fast too, but no source does it, so it waits.

## 6. Question 5: embedding, alignment and dependencies

### bullet's default binary layout

- **Order.** The tensors are written in the order the `save_format` list gives them, concatenated with no per-tensor padding ([weights.rs:161-166](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L161-L166)). For the simple network that is `l0w, l0b, l1w, l1b` ([simple.rs:34-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L34-L39)).
- **Data type and byte order.** "Primitives (e.g. `f32`, `i16`) are always written to files in **little-endian** layout" ([4-saved-networks.md:19](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L19)). The code calls `to_le_bytes` for every value ([weights.rs:300-329](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L300-L329)).
- **Matrix layout.** Every weight is written **column-major**, and an affine layer's weights have shape `output_size × input_size` ([4-saved-networks.md:21-37](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L21-L37)). For `l0w` (N × 768) that puts each feature's N weights together, which is exactly the "one row per feature" layout an accumulator update wants. That is why bullet's reference struct declares `feature_weights: [Accumulator; 768]` ([simple.rs:132-148](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L132-L148)). `.transpose()` writes row-major instead ([4-saved-networks.md:46-51](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L46-L51)).
- **Padding.** `quantised.bin` is "padded to be a multiple of 64 bytes" ([4-saved-networks.md:7](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L7)), using the repeated ASCII bytes `bullet` ([weights.rs:168-177](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L168-L177)). `raw.bin` is unpadded floats ([4-saved-networks.md:8](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md#L8)).
- **No header.** Nothing but the listed tensors and the trailing pad is written. `SavedFormat::custom` exists for adding a header ([weights.rs:195-199](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L195-L199)).

For `(768 → N)x2 → 1` the file is therefore `[i16; 768·N] ++ [i16; N] ++ [i16; 2N] ++ i16 ++ pad to 64`. When `2N` bytes is a multiple of 64, which means N is a multiple of 32, every tensor starts on a 64-byte boundary with no gaps. Then a `#[repr(C)]` struct whose arrays are `align(64)` has **exactly** the file's size and layout: the trailing `i16` plus the struct's tail padding equals bullet's tail pad. That is what makes the `transmute` below compile. My arithmetic, consistent with bullet's own reference struct.

### How engines turn the bytes into a typed struct

| Engine | Mechanism | Alignment | Endianness handling |
|---|---|---|---|
| bullet reference | `static NNUE: Network = unsafe { transmute(*include_bytes!(…)) }` (commented example) | `#[repr(C, align(64))] struct Accumulator { vals: [i16; N] }` | None ([simple.rs:115-120, 183-189](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L115-L120)) |
| akimbo | Same: `unsafe { std::mem::transmute(*include_bytes!("../resources/net.bin")) }` ([network.rs:17-18](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L17-L18)) | `Accumulator` is `#[repr(C, align(64))]` ([:62-66](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs#L62-L66)) | None. `build.rs` copies `EVALFILE` over `resources/net.bin` ([build.rs](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/build.rs)) |
| Carp | `unsafe { mem::transmute(*include_bytes!("../../../bins/net.bin")) }` into `NNUEParams` of `Align64<…>` fields ([nnue/mod.rs:35-49](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs#L35-L49)) | `Align64<T>` wrapper, "also a Viridithas trick" | Its own converter writes `'<h'` little-endian and pads **each** tensor to 64 B ([convert_json.py:16-25](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/convert_json.py#L16-L25)) |
| Viridithas v9 | One `transmute(*include_bytes!(…))` per tensor file ([network.rs:69-83](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L69-L83)) | `#[repr(C, align(64))] struct Align<T>(pub T)` with `Deref` ([:53-67](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L53-L67)) | None |
| Viridithas HEAD | Embeds a **zstd-compressed** file through `include_bytes_aligned!`, an `Align<[u8]>` coerced from `include_bytes!` ([macros.rs:71-78](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/macros.rs#L71-L78); [network.rs:40](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L40)). It decompresses at startup into a 2 MB-padded mmap shared between processes ([network.rs:788-830](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L788-L830)), and keeps a compile-time size assertion for the verbatim case ([:42-45](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L42-L45)) | `#[repr(C, align(64))] Align<T: ?Sized>` ([util.rs:102-103](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/util.rs#L102-L103)) | Its own writer uses `to_le_bytes` ([network.rs:443-445](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L443-L445)) |

`transmute` refuses to compile unless both sizes match exactly, so a network of the wrong size fails the build. But it also reinterprets little-endian bytes as native `i16`. None of these engines checks `target_endian`.

### Viridithas's dependencies, and which ones inference uses

From its [Cargo.toml](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml):

- **Datagen only**, behind the `datagen` feature: `rand`, `serde`, `toml`, `num_cpus`, `chrono`, `ctrlc`, `bulletformat` ([Cargo.toml:22-30, 40-47](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml#L22-L47)).
- **Always on, used by network loading:** `ruzstd` (pure-Rust zstd decoder, the default), `memmap2` (sharing weights between processes), `fxhash` and `anyhow` ([Cargo.toml:54-67](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml#L54-L67); decoder choice at [network.rs:794-797](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L794-L797)). `arrayvec` holds the update index lists ([accumulator.rs:28, 65-66](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/accumulator.rs#L65-L66)).
- **Release only:** the C `zstd` crate, behind `zstd` and `final-release` ([Cargo.toml:31-32, 61](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml#L31-L32)).
- **Not present: `bytemuck`**, nor any SIMD crate.

The zstd step exists because Viridithas's network is huge. Its first layer alone holds `(PSQT + 59808 threat + 4560 pawn-pair) × 1024` weights ([network.rs:51-60, 210-217](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L210-L217)). A `(768 → 256)x2 → 1` file is `(768·256 + 256 + 512 + 1) × 2` bytes, padded: **about 385 KiB**, by my arithmetic. akimbo, Carp and Viridithas v9 all embed uncompressed.

### Options for mudskipper against ADR-0005

ADR-0005 allows no runtime dependencies, and `unsafe` "only in small modules with a stated reason (NNUE SIMD via `std::arch`, and possibly unchecked table lookups…)" ([0005](../adr/0005-dependencies-and-unsafe.md)).

| Option | Dependencies | `unsafe` | Alignment | Endianness | Fit |
|---|---|---|---|---|---|
| **A. `transmute` into an `align(64)` struct** (akimbo, Carp, bullet) | none | yes, one `static` | by type | assumes little-endian; add `const _: () = assert!(cfg!(target_endian = "little"))` | Needs a reason the ADR doesn't list: reinterpreting the embedded network. Zero startup cost; size checked at compile time. |
| **B. Parse at startup with `i16::from_le_bytes` into heap rows of `Align64<[i16; N]>`** | none | **none** | by type (`vec![Align64([0; N]); 768]` is aligned per element) | explicit | Fits the ADR as written. Costs one pass over ~385 KiB at startup (*unverified:* the cost, though it is a single linear pass) and a `OnceLock`/`LazyLock` to hold it. Size check becomes a `const` assert on `include_bytes!(…).len()`. |
| **C. `bytemuck::try_from_bytes`** | +1 (`bytemuck`) | none in mudskipper | **fails at runtime** if the slice isn't aligned ("If the slice isn't aligned for the new type", [docs.rs/bytemuck](https://docs.rs/bytemuck/1.25.2/bytemuck/fn.try_from_bytes.html)), and `include_bytes!` gives only byte alignment, so an aligned wrapper like Viridithas's `include_bytes_aligned!` is still needed | none | Adds a dependency and still needs the wrapper; brings nothing over B. |
| **D. zstd (`ruzstd`)** | +1 or more | — | — | — | Only worth it for multi-megabyte networks; 385 KiB doesn't need it. |

**Building with a network (ADR-0004).** `include_bytes!` needs a literal path, but it accepts `env!("EVALFILE")`. Cargo's `[env]` table can supply a default without `build.rs`: "By default, the variables specified will not override values that already exist in the environment", and `relative = true` resolves the path against the directory that contains `.cargo/` ([Cargo config § env](https://doc.rust-lang.org/cargo/reference/config.html#env)). So `.cargo/config.toml` with `EVALFILE = { value = "<default path>", relative = true }` gives the default path, and a user's `EVALFILE=` overrides it. *Unverified:* that Cargo rebuilds when only the `EVALFILE` value changes. rustc records `env!` reads in its dep-info, but I did not check this against Cargo's docs. A missing file fails at compile time with rustc's own "couldn't read" error. A `compile_error!` can't test for a file's existence, so the "clear message" ADR-0004 asks for would have to come from the Makefile target that fetches the network.

## 7. Recommendation for mudskipper's first network

**Architecture: `(768 → 256)x2 → 1`, perspective, SCReLU.**

- Perspective: the bullet docs and every engine (section 2).
- SCReLU: bullet's default, and Carp measured it ahead of CReLU (+4.7).
- N = 256: what Carp's first network used on 100M positions, gaining +518 over a hand-crafted evaluation, and Carp's log shows wider layers losing until there was much more data. Drop to 128, bullet's example size, if the first dataset is well under 100M positions. Make `HIDDEN` a `const`, a multiple of 32 (section 6), so a resize only means retraining.

**Constants: QA = 255, QB = 64, SCALE = 400**, saved with `.round()` in the order `l0w, l0b, l1w, l1b`, exactly as in [simple.rs:34-39](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs#L34-L39). Keep bullet's default AdamW clip of ±1.98, because the `i16` bounds in section 4 depend on it. Evaluate with `(Σ screlu(a)·w / QA + b) · SCALE / (QA·QB)`, then clamp below `MATE_BOUND`. Revisit SCALE only if fitting the sigmoid to mudskipper's own data lands far from 400. Re-check the eval-based pruning margins after the switch.

**Where the accumulator lives:** in a heap-allocated stack of `MAX_PLY + 1` accumulator pairs owned by `Worker`, next to its existing `stack` of frames ([worker.rs:154-156](../../src/search/worker.rs)). **Not** in `Board`. That keeps `Board` at 216 bytes, `Board::make` unchanged and `chess` free of the network.

**Update strategy: eager and fused, from `enter`.**

1. Give `Worker::enter` the move, so it receives `(parent board, mv)`.
2. A function in `eval` turns `(board, mv)` into at most 2 adds and 2 subs per perspective, following `make`'s cases ([make.rs:33-50](../../src/chess/board/make.rs)):
   - quiet move: sub `from`, add `to`
   - capture: also sub the victim on `captured_square()`, which covers en passant
   - promotion: add the promoted piece rather than the pawn
   - castling: king and rook each move from their square to `castling_king_target` / `castling_rook_target`. Chess960 squares may coincide, which the sub/add form handles.
3. `enter` writes `stack[ply + 1] = stack[ply] − subs + adds` in one pass. A null move copies the parent entry.
4. The root entry is refreshed from scratch at the start of each search.
5. `evaluate` takes the `Worker`'s entry for `ply`, so the three call sites change ([worker.rs:343, 587, 840](../../src/search/worker.rs)).
6. Tests: a proptest over random legal move sequences asserting that the incremental accumulator equals one rebuilt from scratch. Viridithas v9 kept debug-only feature planes for the same check ([network.rs:343-351](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs#L343-L351)).

Moving later to Viridithas-style lazy updates, with a correct flag and a change record per ply, is a **No functional change** speedup checked by NPS (section 3).

**Code: scalar first, no `unsafe`.** Write the accumulator update as plain `zip` loops, as Carp and Viridithas v9 did. Write SCReLU as `i32::from(v * w) * i32::from(v)` with `v = a.clamp(0, QA)`, which is the multiply-before-square trick in safe Rust and exact by the bound in section 4. Measure NPS with `CPU=native`. Add a `#[cfg(target_feature = "avx2")]` `std::arch` module with the scalar version as fallback, akimbo's pattern and already allowed by ADR-0005, only if profiling shows the output layer or update loop is worth it. The SIMD and scalar versions must give identical results, which the Bench signature checks.

**Embedding: option B.** `include_bytes!(env!("EVALFILE"))` with the default set in `.cargo/config.toml` `[env]` (ADR-0004: no `build.rs`). Add a `const` assertion that the length equals the expected padded size. Parse once with `i16::from_le_bytes` into `Box`ed `Align64` rows behind a `LazyLock`.

### Crates and ADR-0005

**No new dependency.** Section 5 explains why neither safe SIMD crate earns its place: both add transitive crates, `wide`/`safe_arch` select at compile time just as `cfg` does, and no reference engine uses either. Section 6 does the same for bytemuck (still needs an alignment wrapper) and zstd (the network is small).

**ADR-0005 needs no amendment** for the recommended path: option B has no `unsafe`, and a later AVX2 module falls under the existing "NNUE SIMD via `std::arch`" exemption. If option A (`transmute`) is chosen instead, the ADR's parenthetical list of reasons must grow. Proposed wording, replacing the sentence that begins "It is allowed only":

> It is allowed only in small modules with a stated reason (NNUE SIMD via `std::arch`, reinterpreting the embedded network's bytes as its typed weights on little-endian targets, and possibly unchecked table lookups that measurably help).

## 8. Postpone for v2

- **King (input) buckets** and **horizontal mirroring** (akimbo, Viridithas; section 3).
- **Finny tables** / bucket accumulator caches (akimbo `EvalTable`, Viridithas `BucketAccumulatorCache`). They are only useful with buckets.
- **Lazy accumulator updates.** A later No functional change speedup (section 3).
- **Output buckets**, bullet's progression step 2 ([2_output_buckets.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/2_output_buckets.rs)), and Viridithas's `OUTPUT_BUCKETS` ([network.rs:116-140](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs#L116-L140)).
- **More than one hidden layer**, pairwise multiplication, `i8` and float later layers ([bullet 4_multi_layer.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression/4_multi_layer.rs); [Viridithas layers.rs:14](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network/layers.rs#L14)). bullet's docs warn these need "manual SIMD and well considered quantisation tech" ([1-basics.md:120-124](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md#L120-L124)).
- **Threat and pawn-pair inputs** (current Viridithas).
- **Hand-written SIMD beyond one AVX2 module**: AVX-512, NEON, register-blocked updates.
- **Runtime CPU detection** (no reference engine does it).
- **zstd-compressed networks, mmap sharing and huge pages** (Viridithas). Only worth it for large networks.
- **Material scaling of the output.** akimbo multiplies the evaluation by `(700 + material/32) / 1024` ([position.rs:280-289](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/position.rs#L280-L289)). That is a search-side tweak that needs its own SPRT.
- **Wider hidden layers** (384 → 512 → 1024), as data grows (Carp's log, section 2).

## 9. Sources

**bullet**: [`jw1912/bullet@5d70177`](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c)
- [docs/1-basics.md](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/1-basics.md)
- [docs/4-saved-networks.md](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/docs/4-saved-networks.md)
- [examples/simple.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/simple.rs)
- [examples/progression/](https://github.com/jw1912/bullet/tree/5d70177680437b7c0ddf958bd4722d95afa78b0c/examples/progression)
- [crates/trainer/src/model/weights.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/model/weights.rs#L161-L335)
- [crates/trainer/src/optimiser/adam.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/trainer/src/optimiser/adam.rs#L19-L29)
- [crates/bullet_lib/src/value.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/value.rs#L60-L115)
- [crates/bullet_lib/src/game/inputs/chess768.rs](https://github.com/jw1912/bullet/blob/5d70177680437b7c0ddf958bd4722d95afa78b0c/crates/bullet_lib/src/game/inputs/chess768.rs)

**Viridithas (HEAD)**: [`cosmobobak/viridithas@4609ff2`](https://github.com/cosmobobak/viridithas/tree/4609ff2a898160f1576fe83c0529a949148541c0)
- [Cargo.toml](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Cargo.toml)
- [build.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/build.rs#L27-L40)
- [Makefile](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/Makefile#L27-L47)
- [src/nnue/network.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/network.rs)
- [src/nnue/accumulator.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/accumulator.rs)
- [src/nnue/simd.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/nnue/simd.rs)
- [src/chess/board/mod.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/chess/board/mod.rs#L1081-L1111)
- [src/evaluation.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/evaluation.rs#L120-L148)
- [src/macros.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/macros.rs#L71-L78)
- [src/util.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/util.rs#L102-L103)

**Viridithas v9.0.0**: [`cosmobobak/viridithas@5d3ef87`](https://github.com/cosmobobak/viridithas/tree/5d3ef87eb7b2e4c2aeff2d66231246325ba80670)
- [src/nnue/network.rs](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/network.rs)
- [src/nnue/accumulator.rs](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/nnue/accumulator.rs)
- [src/board.rs](https://github.com/cosmobobak/viridithas/blob/5d3ef87eb7b2e4c2aeff2d66231246325ba80670/src/board.rs#L1049-L1100)

**akimbo**: [`jw1912/akimbo@f7dd767`](https://github.com/jw1912/akimbo/tree/f7dd7677d6ab9a4a76e3769731084894a7f7ae09)
- [src/network.rs](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/network.rs)
- [src/position.rs](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/position.rs#L10-L289)
- [src/search.rs](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/src/search.rs#L234-L235)
- [build.rs](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/build.rs)
- [Makefile](https://github.com/jw1912/akimbo/blob/f7dd7677d6ab9a4a76e3769731084894a7f7ae09/Makefile)

**Carp**: [`dede1751/carp@7d5065b`](https://github.com/dede1751/carp/tree/7d5065bc26dd16002e6581b25abb3decf62ed86c) (not `Sp00ph/Carp`, which does not exist)
- [chess/src/nnue/mod.rs](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/mod.rs)
- [chess/src/nnue/nnue_history.txt](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_history.txt)
- [chess/src/nnue/nnue_notes.txt](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/nnue_notes.txt)
- [chess/src/nnue/convert_json.py](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/nnue/convert_json.py)
- [chess/src/movegen/make_move.rs](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/chess/src/movegen/make_move.rs#L74-L116)
- [engine/src/position.rs](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/engine/src/position.rs#L89-L121)
- [makefile](https://github.com/dede1751/carp/blob/7d5065bc26dd16002e6581b25abb3decf62ed86c/makefile)

**Crate and tool docs** (read 2026-11-19)
- [bytemuck 1.25.2 `try_from_bytes`](https://docs.rs/bytemuck/1.25.2/bytemuck/fn.try_from_bytes.html)
- [pulp 0.22.3](https://docs.rs/pulp/0.22.3/pulp/)
- [wide 1.7.1](https://docs.rs/wide/1.7.1/wide/)
- [safe_arch 1.2.0](https://docs.rs/safe_arch/1.2.0/safe_arch/)
- [Cargo configuration § `[env]`](https://doc.rust-lang.org/cargo/reference/config.html#env)

**mudskipper** at `f95fa5d`
- [src/chess/board.rs](../../src/chess/board.rs) (`size_of::<Board>()` = 216, measured from a scratch crate)
- [src/chess/board/make.rs](../../src/chess/board/make.rs)
- [src/eval.rs](../../src/eval.rs)
- [src/eval/pesto.rs](../../src/eval/pesto.rs)
- [src/search/worker.rs](../../src/search/worker.rs)
- [src/search/score.rs](../../src/search/score.rs)
- [src/search/tt.rs](../../src/search/tt.rs)
- [Makefile](../../Makefile)
- ADRs [0003](../adr/0003-copy-make-pseudo-legal.md), [0004](../adr/0004-network-outside-repo.md), [0005](../adr/0005-dependencies-and-unsafe.md), [0006](../adr/0006-pesto-until-nnue.md)
