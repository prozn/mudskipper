# Coding standards

Read during review. The baseline is the rust-best-practices skill. This file records Mudskipper's own judgement calls and wins wherever it disagrees with that baseline. Anything fmt, clippy or the compiler enforces is out of scope here.

## Test code is application code

Code under `tests/` (harnesses, helpers, and test bodies) follows application-code conventions rather than public library-API conventions:

- Keep documentation minimal and in plain prose. Do not add `# Panics` or `# Errors` sections to test helpers; panics are an expected way for tests to fail.
- Prefer simple, concrete, local helpers. Avoid abstractions solely to eliminate small amounts of duplication.
- Make assertion failures informative. Prefer `assert_eq!` and `assert_ne!` where appropriate; add custom messages when they provide useful context beyond the default output.
- Keep `Drop` implementations panic-free. Cleanup failures must not cause a second panic during unwinding, which would abort the test process and obscure the original failure.

## Review against the current ticket

Each ticket is one slice of a parent spec (v1 is #1), which lists the planned ticket order. An abstraction that a later ticket introduces (a `Command` enum when `position` and `go` arrive, an input thread, a move picker) is on schedule, so leave it out of findings. Flag what the current ticket's code needs now.
