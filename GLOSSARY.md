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

**Game**:
The current position together with the Zobrist keys of the positions played before it, which **Repetition** detection needs. `ucinewgame` resets it to the start position; `position` replaces it.
_Avoid_: Position (when the history is meant too)

**Repetition**:
A position recurring in a game with the same Zobrist key: same placement, side to move and castling rights, and the same en passant file when a pawn stands beside the pawn that just double-pushed (pins are ignored). Positions before the last capture or pawn move are never compared. `KeyHistory::repetitions` counts earlier occurrences, so a count of 2 is a threefold repetition.
_Avoid_: Draw by repetition (when only one recurrence is meant)

## Search

**Transposition table**:
A table, shared by every search thread, that remembers results from positions already searched, keyed by Zobrist key. Reaching the same position by a different move order then reuses that work. Its size is set by the `Hash` option.
_Avoid_: Hash table, hash (except as the UCI option's name), TT (in prose)

**Pruning**:
Skipping a move or a whole node without searching it, because it is judged unable to change the result.
_Avoid_: Culling, cutting (ambiguous with a beta cutoff)

**Reduction**:
Searching a move, or a whole node, to a shallower depth than usual because it is unlikely to matter. A move reduced below its siblings' depth is searched again at full depth if it turns out better than expected.
_Avoid_: Culling, pruning (a reduced move is still searched)

**Noisy move**:
A capture, including en passant, or a promotion, whether or not the promotion captures. These are the moves searched until a position is quiet.
_Avoid_: Tactical move, forcing move (suggests checks), capture (leaves out non-capturing promotions)

**Quiet move**:
A move that neither captures nor promotes, including castling. Every move is either a **Noisy move** or a quiet one.
_Avoid_: Non-capture (includes non-capturing promotions), silent move

**Killer move**:
A **Quiet move** that recently caused a beta cutoff at the same ply of the current search, tried in sibling nodes straight after the **Noisy moves**. Two are kept per ply, and they are forgotten at the start of each search.

**History**:
A score for each **Quiet move**, by colour, origin and destination square, that rises when the move causes a beta cutoff and falls when it was tried before another quiet move did. Quiet moves other than the **Killer moves** are searched in order of it. It is kept between searches and forgotten on `ucinewgame`.
_Avoid_: History table (except for the butterfly array itself), key history (that is the record of earlier positions for **Repetition**)

**Principal variation search**:
Searching the first move at a node with the full window and every later move with a null window just above alpha, which only proves the move is no better. At a node on the principal variation, a move whose null-window score lands inside the window is searched again with the full window for its exact score.
_Avoid_: PVS (in prose), NegaScout, scout search

**Aspiration window**:
A narrow window around the previous iteration's score, used to search the root from a modest depth onward. When the score falls outside it, the window is widened on that side and the root searched again until the score fits.

**Check extension**:
Searching a node one ply deeper than usual because the side to move is in check, so a checking line is not cut off before its point is seen. In practice this extends the checking move by one ply. Lines still end at the search's deepest ply.

**Null-move pruning**:
**Pruning** a node off the principal variation by letting the side to move pass: if a reduced-depth search after the pass still fails high, the node returns the cutoff without searching its moves. It is tried only when the static evaluation is at least beta, and not in check, straight after another pass, or when the side to move has only its king and pawns, where zugzwang makes passing unsound.

**Late move reduction**:
**Reduction** of a **Quiet move** searched late at a node, after the first few moves, by a number of plies that grows with the logarithms of the depth and the move's number. The TT move, **Killer moves**, moves that give check and every move from a node in check are not reduced, and moves with good **History** or on the principal variation are reduced less. A reduced move that beats alpha is searched again at full depth.
_Avoid_: LMR (in prose)

**Reverse futility pruning**:
**Pruning** a shallow node off the principal variation, not in check, whose static evaluation exceeds beta by a margin that grows with depth: the node returns its static evaluation without searching its moves. It is not tried when beta is a mate score.
_Avoid_: Static null-move pruning, RFP (in prose)

**Futility pruning**:
**Pruning** the remaining **Quiet moves** at a shallow node off the principal variation, not in check, whose static evaluation falls short of alpha by more than a margin that grows with depth, since no quiet move is likely to make up the difference. It starts only once a move has been searched with a score that is not a mate, so a node is never left with no move searched. **Noisy moves** are still searched.
_Avoid_: FP (in prose)

**Late move pruning**:
**Pruning** the remaining **Quiet moves** at a shallow node off the principal variation, not in check, once a number of moves that grows with depth has been searched, the best of them with a score that is not a mate. **Noisy moves** are still searched.
_Avoid_: Move count pruning, LMP (in prose)
