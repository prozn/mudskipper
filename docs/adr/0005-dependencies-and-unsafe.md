# Dependencies only when chosen deliberately; `unsafe` only with a documented reason

A dependency is allowed when it is chosen deliberately: its entry in `Cargo.toml` carries a comment giving its reason, written in the PR that brings it in. Pure-Rust, small, widely used crates are preferred. This keeps builds fast and reproducible and keeps code we haven't read out of the hot path, without a blanket ban standing in the way of a crate that earns its place. Crates used only by datagen sit behind the `datagen` Cargo feature, so a normal build never compiles them. There is no runtime crate yet, so error types are still written by hand, without `thiserror`.

`unsafe` is denied crate-wide. It is allowed only in small modules with a stated reason (NNUE SIMD via `std::arch`, and possibly unchecked table lookups that measurably help). Each such module carries `#![deny(unsafe_op_in_unsafe_fn)]`, and every block carries a `// SAFETY:` comment.
