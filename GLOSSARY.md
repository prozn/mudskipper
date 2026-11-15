# Mudskipper

A UCI chess engine. This glossary fixes the vocabulary used across code, issues, commits and test reports.

## Testing and measurement

**SPRT**:
A sequential probability ratio test run as engine-vs-engine games to decide whether a change gains Elo, within chosen error bounds. Run manually, outside this repo.
_Avoid_: Elo test, match

**Bench**:
A fixed, deterministic search over a fixed set of positions whose total node count identifies the engine's search behaviour.
_Avoid_: Benchmark (ambiguous with micro-benchmarks), speedtest

**Bench signature**:
The node count reported by **Bench**, recorded in each commit message as `Bench: <nodes>`. A change that leaves it unchanged is functionally identical.
_Avoid_: Node signature, bench hash

**No functional change**:
A change that leaves the **Bench signature** identical, declared as such in its commit message and verified by CI. It needs no **SPRT**.
_Avoid_: Refactor (as a claim of equivalence), NFC

**Perft**:
A count of leaf nodes in the full legal-move tree from a position to a given depth, used to verify move generation.
_Avoid_: Move count test

## Chess

**Chess960**:
The variant in which the back-rank pieces start in shuffled positions; the engine treats standard chess as one Chess960 starting position.
_Avoid_: Fischer Random, FRC (in code)

**DFRC**:
Double Fischer Random Chess: Chess960 where White and Black have independently shuffled back ranks. Used as an opening source for **SPRT**.

**X-FEN**:
A FEN castling notation that writes `KQkq` for the outermost rook on each side of the king and the rook's file (`A`-`H`, `a`-`h`) for any other rook. Standard positions print as standard FEN; `Board::to_fen` uses it.

**Shredder-FEN**:
A FEN castling notation that always writes the rook's file, such as `HAha` for the standard start position. `d` prints it when `UCI_Chess960` is on.

**Repetition**:
A position recurring in a game with the same Zobrist key: same placement, side to move, castling rights and available en passant capture. Positions before the last capture or pawn move are never compared. `KeyHistory::repetitions` counts earlier occurrences, so a count of 2 is a threefold repetition.
_Avoid_: Draw by repetition (when only one recurrence is meant)
