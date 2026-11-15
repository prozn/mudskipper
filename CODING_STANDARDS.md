# Coding standards

Read during review. The baseline is the rust-best-practices skill. This file records Mudskipper's own judgement calls and wins wherever it disagrees with that baseline. Anything fmt, clippy or the compiler enforces is out of scope here.

## Test code is application code

Code under `tests/` (harnesses, helpers, test bodies) follows the application-code side of the baseline, not the library-API rules:

- Write docs as plain prose. `# Panics` and `# Errors` sections are unnecessary: a test helper panicking is how a test fails.
- Keep helpers concrete and local. Small repeated shapes (a deadline loop, a `try_wait` call) can stay repeated when extracting them would add indirection.
- Give every assertion a message with the values involved, so a CI failure reads without a rerun.
- Keep `Drop` impls panic-free. A second panic while a failing test unwinds aborts the whole test binary and hides the original failure.

## Review against the current ticket

Each ticket is one slice of a parent spec (v1 is #1), which lists the planned ticket order. An abstraction that a later ticket introduces (a `Command` enum when `position` and `go` arrive, an input thread, a move picker) is on schedule, so leave it out of findings. Flag what the current ticket's code needs now.
