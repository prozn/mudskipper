# NNUE networks live outside this repo and builds stay offline

Network files are published in a separate repository and supplied at build time. They are fetched by hand or by a Makefile target into a default path, or pointed to with `EVALFILE`, then embedded with `include_bytes!`. `build.rs` never touches the network. Downloading in `build.rs` (Reckless's approach) and committing nets or using git-lfs were rejected. The goals are builds that work offline and give the same result every time, and a repo that holds only engine code. A build with no network available fails with a clear message.
