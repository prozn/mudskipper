# Dependencies only when listed here; `unsafe` only with a documented reason

A dependency is allowed when it is chosen deliberately: it is listed below with its reason, and it is added to the list in the PR that brings it in. Pure-Rust, small, widely used crates are preferred. This keeps builds fast and reproducible and keeps code we haven't read out of the hot path, without a blanket ban standing in the way of a crate that earns its place. Crates used only by datagen sit behind the `datagen` Cargo feature, so a normal build never compiles them. No runtime crate is listed yet, so error types are still written by hand, without `thiserror`.

`unsafe` is denied crate-wide. It is allowed only in small modules with a stated reason (NNUE SIMD via `std::arch`, and possibly unchecked table lookups that measurably help). Each such module carries `#![deny(unsafe_op_in_unsafe_fn)]`, and every block carries a `// SAFETY:` comment.

## Approved crates

| Crate | Kind | Reason |
|---|---|---|
| `criterion` | dev | Benchmarks (`benches/`). |
| `proptest` | dev | Property tests. |
| `viriformat` | dev | Tests the datagen writer against the decoder bullet uses. Pinned to `=2.0.1`, the version bullet pins. Arrives with the datagen command (#79). |
