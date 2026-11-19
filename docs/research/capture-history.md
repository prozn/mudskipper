# Capture history for move ordering

Research for adding capture history to mudskipper's move ordering. Written 2026-11-18 against mudskipper at `afdd37b`.

Each claim links to engine source at a pinned commit, or to the Chess Programming Wiki (CPW). The engines are far stronger than mudskipper, and each capture history term interacts with many other terms that mudskipper does not have. Section 5 separates what a first version needs from later refinements.

Pinned sources (see the [Sources](#6-sources) list):

| Engine | Commit | Language |
|---|---|---|
| Stockfish | `5d03b6e` (2026-11-15) | C++ |
| Viridithas | `4609ff2` (2026-11-17) | Rust |
| Stormphrax | `d1468d9` (2026-08-29) | C++ |
| Berserk | `1d2354d` (2026-11-14) | C |
| Ethereal | `0e47e9b` (2024-06-11) | C |
| Obsidian | `720614a` (2025-09-27) | C++ |

Terms follow [GLOSSARY.md](../../GLOSSARY.md): **noisy move** (capture or promotion), **quiet move**, **history**, **static exchange evaluation** (SEE in code and tables only). "Capture history" here means a history table for noisy moves. Viridithas calls it "tactical history" and Stormphrax "noisy history".

## 1. Summary

- **Indexing.** Five of the six engines index by moved piece (with colour), destination square and captured piece type, the CPW's "moved piece, target square, and captured piece type" ([CPW](https://www.chessprogramming.org/History_Heuristic#Capture_History)). Viridithas, Berserk and Ethereal add one or two threat bits. Stormphrax alone uses `[from][to][captured piece][defended]`. Every engine stores `i16`. The tables take 15–21 KiB, except Stormphrax's at 208 KiB.
- **Combining with MVV.** Every engine drops the least-valuable-attacker term and uses **victim value × scale + capture history**. Measured as "a full history is worth N pawns of victim value", N runs from about 7 (Stockfish) to about 21 (Stormphrax). Obsidian uses mudskipper's scale exactly: pawn = 100, victim × 16, history max 16384. Only Ethereal still splits good from bad noisy moves at plain SEE ≥ 0. Stockfish, Stormphrax, Berserk and Obsidian use the threshold `SEE ≥ −score / K`, where `score` is the ordering score itself. Viridithas uses a fixed negative bound.
- **Updates.** Berserk, Ethereal and Obsidian reuse the quiet history bonus formula. Stockfish shares the base formula but scales it separately for captures. Viridithas and Stormphrax tune separate constants, and Stormphrax even uses a different malus depending on whether a quiet or a noisy move was best. In **all six**, every noisy move tried before the best move gets the malus, **whether the best move was quiet or noisy**. Quiet moves get the malus only when the best move was quiet. Gravity is the same `v += b − v·|b|/MAX` as mudskipper's. MAX is 16384 or close to it everywhere except Stockfish (10692).
- **Use outside ordering.** Stockfish, Viridithas, Stormphrax, Berserk and Ethereal all use capture history in late move reductions of noisy moves. Stockfish, Viridithas, Stormphrax and Ethereal add it to the SEE pruning margin. Stockfish adds it to a capture futility margin. Stormphrax prunes noisy moves on very low capture history. None of these exist in mudskipper yet, since it reduces and prunes only quiet moves, so they all come later.
- **Recommendation.** Index `[colour][piece type][to][victim type]` as `i16` (9 KiB) with the existing gravity and `MAX_HISTORY`. Reuse `bonus(depth)` for both bonus and malus. Score noisy moves `16 × ORDER_VALUES[victim] + capture history`. Keep plain SEE ≥ 0 for the split. In the first SPRT, give the malus to the noisy moves tried before any beta cutoff. Section 5 lists what to test next.

## 2. Question 1: indexing, size and element type

| Engine | Index | Entries × type | Size | Source |
|---|---|---|---|---|
| Stockfish | `[piece 16][to 64][captured type 8]` | 8192 × `i16` | 16 KiB | [history.h:133-134](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/history.h#L133-L134), [types.h:206-207](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/types.h#L206-L207) |
| Obsidian | `[piece 16 × to 64][captured type 8]` | 8192 × `i16` | 16 KiB | [history.h:15](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/history.h#L15) |
| Viridithas | `[to threatened 2][captured type 6][piece 12][to 64]` | 9216 × `i16` | 18 KiB | [historytable.rs:59-62, 152-155](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/historytable.rs#L152-L155), [history.rs:165-182](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/history.rs#L165-L182) |
| Berserk | `[piece 12][to 64][to defended 2][captured type 7]` | 10752 × `i16` | 21 KiB | [thread.h:69](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/thread.h#L69), [history.h:50-58](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.h#L50-L58) |
| Ethereal | `[piece type 6][from threatened 2][to threatened 2][to 64][captured type 5]` | 7680 × `i16` | 15 KiB | [types.h:104](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/types.h#L104), [history.c:51-64](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L51-L64) |
| Stormphrax | `[from 64][to 64][captured piece 12 + 1][to defended 2]` | 106496 × `i16` | 208 KiB | [history.h:215-217](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/history.h#L215-L217) |

Notes:

- **Piece-to is the common shape.** "Piece" includes colour in Stockfish, Obsidian, Viridithas and Berserk, so the side to move is implicit. Ethereal indexes by piece type only, with no colour ([history.c:54](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L54)). Stormphrax's from-to table has no colour axis either, but its captured piece is coloured, which implies the side to move ([history.h:215-217](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/history.h#L215-L217); `captureTarget` returns the coloured piece, [position.cpp:669-681](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/position.cpp#L669-L681)).
- **No engine found uses `[colour][from][to][victim]`.** That would be 2 × 64 × 64 × 6 = 49152 entries (96 KiB). Stormphrax is the only from-to design, and it adds the defended bit.
- **En passant** maps to a pawn victim in every engine checked ([Viridithas history.rs:281-294](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/history.rs#L281-L294), [Berserk history.h:57](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.h#L57), [Ethereal history.c:44-49](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L44-L49), [Stormphrax position.cpp:676-677](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/position.cpp#L676-L677)).
- **Promotions differ:**

  | Engine | Non-capturing promotion's victim slot | Capturing promotion's victim slot |
  |---|---|---|
  | Viridithas | Pawn | Pawn (all promotions) |
  | Ethereal | Pawn | Pawn (every non-normal move) |
  | Stockfish | empty-square slot (type 0); only queen promotions are noisy | captured type |
  | Stormphrax | extra 13th "none" slot | captured piece |

  Viridithas explains why the pawn slot is safe: "you'd never usually capture pawns on the back ranks, so these slots are free" ([history.rs:281-288](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/history.rs#L281-L288)). Ethereal: [history.c:44-49](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L44-L49). Stockfish's `capture_stage` counts only queen promotions as noisy: [position.h:369-380](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/position.h#L369-L380), with the empty-square lookup at [movepick.cpp:227-231](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/movepick.cpp#L227-L231). Stormphrax: [history.h:216](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/history.h#L216).
- **Origin.** The CPW credits capture history to Stefan Geschwentner in 2016 ([CPW](https://www.chessprogramming.org/History_Heuristic#Capture_History)). *Unverified:* I did not trace this to the Stockfish commit.

## 3. Question 2: combining with MVV, and the good/bad split

### Ordering score

| Engine | Noisy move score | MVV per pawn | Ratio: history max ÷ one pawn of MVV | Source |
|---|---|---|---|---|
| Stockfish | `capthist + 7 × PieceValue[victim]` (pawn 208, knight 781, bishop 825, rook 1276, queen 2538) | 1456 | 10692 / 1456 ≈ 7.3 | [movepick.cpp:229-231](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/movepick.cpp#L229-L231), [types.h:196-200](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/types.h#L196-L200) |
| Obsidian | `16 × PIECE_VALUE[victim] + 16384 × isPromotion + capthist` (pawn 100, knight 370, bishop 390, rook 610, queen 1210) | 1600 | 16384 / 1600 ≈ 10.2 | [movepick.cpp:111-117](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/movepick.cpp#L111-L117), [types.h:215](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/types.h#L215) |
| Viridithas | `MVV_SCORE[victim] + tactical` with MVV `[0, 2400, 2400, 4800, 9600, 0]`; pawn victims get 0 | n/a (minor = 2400) | 16383 / 2400 ≈ 6.8 minor-piece steps | [movepicker.rs:358-379](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/movepicker.rs#L358-L379) |
| Ethereal | `64000 + capthist + 64000 × isQueenPromotion + MVVAugment[victim]` with `[0, 2400, 2400, 4800, 9600]` | n/a (minor = 2400) | 16384 / 2400 ≈ 6.8 minor-piece steps | [history.c:133-151](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L133-L151) |
| Berserk | `16 × capthist / 256 + SEE_VALUE[victim]` (pawn 86, queen 1047) | 86 | (16384 / 16) / 86 ≈ 11.9 | [movepick.c:78-80](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/movepick.c#L78-L80), [tune.h:96-100, 126](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h#L96-L100) |
| Stormphrax | `noisyHist / 8 + seeValue[victim]` (+ queen−pawn for a promotion); pawn 97 | 97 | (16287 / 8) / 97 ≈ 21 | [movepick.cpp:217-233](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/movepick.cpp#L217-L233), [tunable.h:155-159](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L155-L159) |

Every engine **adds** history to a scaled victim value. None **replaces** MVV, and none keeps an attacker term. This matches the CPW: "The history values is used as a replacement for LVA in MVV-LVA" ([CPW](https://www.chessprogramming.org/History_Heuristic#Capture_History)). A saturated history can move a capture ahead of one or more higher-value victims everywhere, but values rarely sit at the cap.

### Good/bad split

| Engine | A noisy move is "good" (searched before quiets) when | Equivalent in victim value and history | Source |
|---|---|---|---|
| Ethereal | `SEE ≥ 0` (plain) | — | [movepicker.c:56, 119-123](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/movepicker.c#L119-L123) |
| Viridithas | `SEE ≥ −93` in the main search, `SEE ≥ −141` in quiescence (constant) | — | [search.rs:81-82, 1311](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L81-L82), [search.rs:648-652](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L648-L652) |
| Stockfish | `SEE ≥ −score / 18` | `−(0.39 × PieceValue[victim] + capthist / 18)` | [movepick.cpp:311-317](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/movepick.cpp#L311-L317) |
| Berserk | `SEE ≥ −score × 32 / 64` | `−(0.5 × SEE_VALUE[victim] + capthist / 32)` | [movepick.c:116](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/movepick.c#L116), [tune.h:126-127](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h#L126-L127) |
| Obsidian | main search: `SEE ≥ −score / 32`; quiescence: a fixed `seeMargin` | `−(0.5 × PIECE_VALUE[victim] + capthist / 32)` | [movepick.cpp:155-160](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/movepick.cpp#L155-L160) |
| Stormphrax | `SEE ≥ −score / 4 + 72` | `−(0.25 × seeValue[victim] + noisyHist / 32) + 72` | [movepick.cpp:59-61](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/movepick.cpp#L59-L61), [tunable.h:186](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L186) |

So four of six use `SEE ≥ −score / K`. Since `score` contains the victim value, this threshold is roughly "may lose up to a quarter to half of the victim's value, plus history/32". Berserk, Obsidian and Stormphrax all land on `history / 32`. Stockfish uses `history / 18`.

## 4. Question 3: update rules

### Bonus and malus

| Engine | Capture bonus | Capture malus | Shared with quiet history? | MAX (gravity) | Source |
|---|---|---|---|---|---|
| Stockfish | `b × 1427/1024`, where `b = min(133d − 81, 1487) + 364·[best is TT move] + prevStatScore/28`, then `b += b·(nQuiets + nCaptures)/256` | `min(968d − 235, 2244) × 1489/1024` | Same base `b` and malus; quiet uses `×899/1024` and `×1159/1024` with a per-move decay of 921/1024 | 10692; table filled with −742 on clear | [search.cpp:1999-2037](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1999-L2037), [history.h:68-77, 133-134](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/history.h#L68-L77), [search.cpp:715](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L715) |
| Viridithas | `min(104d + 328, 1248)` | `min(29d + 394, 1122)` | Separate (main history: `min(357d + 226, 2241)` / `min(111d + 561, 915)`) | 16383 (`i16::MAX / 2`), then clamped | [search.rs:103-108](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L103-L108), [historytable.rs:7-57](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/historytable.rs#L7-L57) |
| Stormphrax | `clamp(274d′ − 469, 0, 2355)` | best noisy: `clamp(319d′ − 170, 0, 1777)`; best quiet: `clamp(371d′ − 78, 0, 1629)` | Separate (quiet: `clamp(339d′ − 313, 0, 2663)` / `clamp(384d′ − 177, 0, 1318)`); `d′ = d + [not in check and eval ≤ best]` | 16287 | [tunable.h:312-335](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L312-L335), [history.h:54-65](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/history.h#L54-L65), [search.cpp:1339-1397](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1339-L1397) |
| Berserk | `min(1708, 4d² + 191d − 118)` | the same magnitude, negated | Shared bonus; quiet malus is softened by `+36·(nQuiets − 1)` | 16384 | [history.h:73-81](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.h#L73-L81), [tune.h:103-107](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h#L103-L107), [history.c:40-92](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.c#L40-L92) |
| Ethereal | `d > 13 ? 32 : 16d² + 128·max(d − 1, 0)` | the same, negated | Shared | 16384 | [history.c:31-41](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L31-L41), [history.h:25](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.h#L25) |
| Obsidian | `min(175d + 15, 1409)` | `min(196d − 25, 1047)` | Shared (`statBonus` / `statMalus`) | 16384 | [search.cpp:28-35, 220-226](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/search.cpp#L220-L226), [history.h:46-48](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/history.h#L46-L48) |

All six use the gravity update `v += b − v·|b| / MAX`, the same formula as mudskipper's `History::update` ([history.rs](../../src/search/history.rs)).

### Which moves get the bonus and malus

| Engine | When updated | Best move is quiet | Best move is noisy | Source |
|---|---|---|---|---|
| Stockfish | a best move exists (alpha raised) | quiet bonus/malus; **malus to all noisy moves searched** (up to 32 moves) | capture bonus to the best move; malus to the other noisy moves searched; quiet history unchanged | [search.cpp:1576-1581, 1597-1602](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1597-L1602), [search.cpp:2007-2037](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L2007-L2037) |
| Viridithas | alpha raised | **malus to all noisy moves tried** | bonus to the best move; malus to the others | [search.rs:1645-1674](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1645-L1674) |
| Stormphrax | a best move exists (alpha raised) | **malus to the noisy moves that failed low** (best-quiet constants) | bonus; malus to the noisy moves that failed low (best-noisy constants) | [search.cpp:1302-1307, 1323-1329, 1339-1397](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1339-L1397) |
| Berserk | beta cutoff, including in quiescence at depth 1 | **malus to all captures tried** | bonus; malus to the other captures | [search.c:941-951, 1088-1091](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/search.c#L941-L951), [history.c:80-92](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.c#L80-L92) |
| Ethereal | beta cutoff | **malus to all captures tried** | bonus; malus to the others | [search.c:780-787](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/search.c#L780-L787), [history.c:153-162](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L153-L162) |
| Obsidian | beta cutoff | **malus to all captures tried** | bonus; malus to the others | [search.cpp:1155-1164, 1180-1214](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/search.cpp#L1180-L1214) |

Viridithas's comment gives the reasoning: "tactical moves ought to be good in any position, so it's good to decrease tactical history scores even when the best move was non-tactical" ([search.rs:1668-1671](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1668-L1671)).

Extra updates (refinements, not core):

- **Prior-capture bonus on fail low.** If a node fails low and the parent's move was a capture, that capture gets a fixed bonus: Stockfish 892 ([search.cpp:1632-1640](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1632-L1640)), Stormphrax 59 ([search.cpp:1422-1423](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1422-L1423), [tunable.h:355](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L355)).
- **Updates in quiescence:** only Berserk, at depth 1 ([search.c:1088-1091](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/search.c#L1088-L1091)).

## 5. Question 4: use outside ordering

| Use | Stockfish | Viridithas | Stormphrax | Berserk | Ethereal |
|---|---|---|---|---|---|
| LMR of noisy moves | `statScore = 873·PieceValue[victim]/128 + capthist`; `r −= statScore·439/4096` ([search.cpp:1372-1382](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1372-L1382)) | `stat = tactical·43/32`; `r −= stat·1024/17017`; noisy moves reduced like quiets ([search.rs:1396-1417, 1523](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1396-L1417)) | separate noisy LMR table; `r −= hist·423/4096` ([search.cpp:1004, 1145-1170](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1145-L1170), [tunable.cpp:48-57](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.cpp#L48-L57)) | `R −= 6144·hist/65536`; captures at PV nodes not reduced ([search.c:751-756, 839](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/search.c#L751-L756)) | `R = 3 − hist/4952` ([search.c:705-713](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/search.c#L705-L713)) |
| SEE pruning margin | prune if `!see_ge(−(177d + capthist·34/1024))` ([search.cpp:1221-1226](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1221-L1226)) | threshold `−28d² − stat·25/1024` ([search.rs:1187-1191, 1471-1485](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1471-L1485)) | threshold `min(−111d − hist/65, 0)` ([search.cpp:1054-1059](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1054-L1059)) | no history; depth table only ([search.c:774-777](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/search.c#L774-L777)) | threshold `seeMargin[noisy] − hist/128` ([search.c:636-642](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/search.c#L636-L642)) |
| Futility for captures | prune if `eval + 234 + 247·lmrDepth + PieceValue[victim] + 134·capthist/1024 ≤ alpha`, lmrDepth < 8, no check ([search.cpp:1206-1219](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1206-L1219)) | quiet only ([search.rs:1457-1468](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1457-L1468)) | quiet only ([search.cpp:1042-1047](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1042-L1047)) | quiet only | quiet only |
| History pruning of noisy moves | no (quiet continuation history only) | quiet only ([search.rs:1446-1455](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1446-L1455)) | skip a noisy move if `depth ≤ 4 && hist < −1006d² − 1121` ([search.cpp:1049-1052](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L1049-L1052), [tunable.h:243-244](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L243-L244)) | quiet only | quiet only |

Obsidian was checked only for indexing, scoring, the split and updates. *Unverified:* its pruning and LMR use of capture history.

mudskipper today: late move reduction applies only to quiet moves, and futility and late move pruning skip only quiet moves ([GLOSSARY.md](../../GLOSSARY.md)). There is no SEE pruning in the main search; quiescence uses `MovePicker::good_noisy`, which skips bad noisy moves out of check ([worker.rs:578-623](../../src/search/worker.rs)). So every row above would be a new feature for mudskipper, not a change to an existing one.

## 6. Recommended first version for mudskipper

Aim: one SPRT that isolates "order noisy moves by victim value plus capture history instead of MVV-LVA".

### Table

- A new `CaptureHistory` in [history.rs](../../src/search/history.rs), indexed `[colour][moved piece type][to][victim type]` as `Box<[[[[i16; 6]; 64]; 6]; 2]>`. That is 4608 entries, or 9 KiB. It matches the Stockfish/Obsidian/Berserk/Viridithas piece-to shape, with colour as its own axis, the way mudskipper's `History` keeps it.
  - Prefer it to `[colour][from][to][victim]` (96 KiB, about 10× sparser). Only Stormphrax uses a from-to shape, and it does not use this one. A smaller table also learns faster from few updates.
- Victim index: the captured piece type, with en passant as `Pawn`. A **non-capturing promotion** uses the `Pawn` slot, like Viridithas and Ethereal. That slot is never otherwise used for a pawn moving to the back rank.
- Store `i16`, use the same gravity formula and `MAX_HISTORY = 16_384`. Move the formula into a shared helper so both tables use it. That refactor can land first as a **no functional change** commit, checked by the **Bench signature**.
- Keep it between searches and forget it on `ucinewgame`, the same as `History` ([worker.rs:141-149](../../src/search/worker.rs)).

### Constants

- Bonus = malus = the existing `bonus(depth) = min(300·depth − 250, 1500)`. Berserk, Ethereal and Obsidian share their quiet formula, and sharing means no new constants in the first SPRT.

### Scoring and the split

- `score = 16 × ORDER_VALUES[victim] + capture_history(mv)`. Drop the attacker term.
  - The ×16 with pawn = 100 and MAX 16384 is exactly Obsidian's scale ([movepick.cpp:114-117](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/movepick.cpp#L111-L117)). A full history is worth about 10 pawns of victim value, in the 7–21 range of the other engines.
- Keep the promotion term in the same proportion to the victim term as now. Today the victim weight is 10 and the promotion weight is 1. At victim weight 16, use `16 × ORDER_VALUES[promotion] / 10`, so that promotions keep their place relative to captures. Changing their relative place is a separate test.
- Keep **plain `see_ge(mv, 0)`** for the split. The same moves stay good or bad; only the order inside each group changes. Bad noisy moves are deferred in pick order, so they also follow the new score. Ethereal still uses plain SEE ≥ 0, so this is a known-viable stopping point.
- `MovePicker::good_noisy` (quiescence) uses the same score, so quiescence ordering changes too. Make no capture history updates in quiescence at first.

### Updates

These go in `record_cutoff`, on a beta cutoff only, as mudskipper does today. The search loop needs a second list, `noisy_tried`, next to `quiets_tried`.

| Cut-off move | Quiet history | Capture history |
|---|---|---|
| Quiet | as now: bonus to the move, malus to `quiets_tried` | **malus to every noisy move in `noisy_tried`** |
| Noisy | unchanged; no killer either (as now) | bonus to the cut-off move; malus to the other noisy moves in `noisy_tried` |

All six engines do both rows. Leaving out the quiet-cut-off row would be a deviation needing its own test, not a simplification.

### What to SPRT, in order

1. **Capture history ordering** (above): table, both update rows, `16 × victim + capthist`, plain SEE ≥ 0.
2. **Tuning the first version**, one at a time:
   - victim weight 8 / 16 / 32;
   - separate capture bonus/malus constants. Viridithas and Stormphrax tuned the capture malus and bonus away from the quiet ones (§4).
3. **History-adjusted good/bad split:** `see_ge(mv, −score / 32)`, i.e. `−(victim/2 + capthist/32)`. This is Obsidian's formula at the same scale, and Berserk's and Stormphrax's history term (§3). Stockfish's `/18` is a later variant.
4. **Update on alpha raise, not only on beta cutoff**, as in Stockfish, Viridithas and Stormphrax (§4). This also changes quiet history, so test it on its own.
5. **Threat bit:** whether the destination square is attacked or defended (Viridithas, Berserk, Stormphrax, Ethereal). This needs an opponent attack bitboard per node, which mudskipper may not keep yet.
6. **Search uses**, each a new feature on its own:
   - LMR of late bad noisy moves, with a history term;
   - SEE pruning of noisy moves with a `− capthist / K` margin;
   - capture futility (Stockfish only);
   - noisy history pruning (Stormphrax only).
7. **Small extras:**
   - prior-capture bonus on fail low (Stockfish, Stormphrax);
   - Stockfish's non-zero fill value on clear;
   - quiescence updates (Berserk).

The bad noisy LMR work shelved on branch `lmr-bad-noisy` (#53) would fit at step 6. It may combine with capture history through a history term in its reduction.

## 7. Sources

All links are pinned to the commits below.

**Stockfish** — [`official-stockfish/Stockfish@5d03b6e`](https://github.com/official-stockfish/Stockfish/tree/5d03b6e578a5eedc822e4563bdbb89e762fc2a59)
- [src/history.h](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/history.h#L68-L134)
- [src/movepick.cpp](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/movepick.cpp#L185-L330)
- [src/search.cpp](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/search.cpp#L1984-L2037)
- [src/types.h](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/types.h#L196-L220)
- [src/position.h](https://github.com/official-stockfish/Stockfish/blob/5d03b6e578a5eedc822e4563bdbb89e762fc2a59/src/position.h#L369-L380)

**Viridithas** — [`cosmobobak/viridithas@4609ff2`](https://github.com/cosmobobak/viridithas/tree/4609ff2a898160f1576fe83c0529a949148541c0)
- [src/history.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/history.rs#L165-L294)
- [src/historytable.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/historytable.rs#L1-L195)
- [src/movepicker.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/movepicker.rs#L205-L379)
- [src/search.rs](https://github.com/cosmobobak/viridithas/blob/4609ff2a898160f1576fe83c0529a949148541c0/src/search.rs#L1396-L1740)

**Stormphrax** — [`Ciekce/Stormphrax@d1468d9`](https://github.com/Ciekce/Stormphrax/tree/d1468d99e3d3733100d20d682ac627998cce94b2)
- [src/history.h](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/history.h#L36-L257)
- [src/movepick.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/movepick.cpp#L50-L233)
- [src/search.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/search.cpp#L997-L1425)
- [src/tunable.h](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/tunable.h#L155-L355)
- [src/position.cpp](https://github.com/Ciekce/Stormphrax/blob/d1468d99e3d3733100d20d682ac627998cce94b2/src/position.cpp#L669-L681)

**Berserk** — [`jhonnold/berserk@1d2354d`](https://github.com/jhonnold/berserk/tree/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49)
- [src/history.h](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.h#L40-L81)
- [src/history.c](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/history.c#L29-L92)
- [src/movepick.c](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/movepick.c#L70-L120)
- [src/search.c](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/search.c#L745-L951)
- [src/tune.h](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/tune.h#L96-L127)
- [src/thread.h](https://github.com/jhonnold/berserk/blob/1d2354d3250e5ed6fe832d0c6f60eb0b684d3e49/src/thread.h#L69)

**Ethereal** — [`AndyGrant/Ethereal@0e47e9b`](https://github.com/AndyGrant/Ethereal/tree/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a)
- [src/history.c](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c#L31-L162)
- [src/movepicker.c](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/movepicker.c#L50-L135)
- [src/search.c](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/search.c#L585-L787)
- [src/types.h](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/types.h#L104)

**Obsidian** — [`gab8192/Obsidian@720614a`](https://github.com/gab8192/Obsidian/tree/720614a61ed6201037516d555f0bd2f7e5aeae3c)
- [src/history.h](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/history.h#L15-L48)
- [src/movepick.cpp](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/movepick.cpp#L108-L160)
- [src/search.cpp](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/search.cpp#L1155-L1214)
- [src/types.h](https://github.com/gab8192/Obsidian/blob/720614a61ed6201037516d555f0bd2f7e5aeae3c/src/types.h#L215)

**Chess Programming Wiki** (read 2026-11-18, served as a read-only recovery copy)
- [History Heuristic § Capture History](https://www.chessprogramming.org/History_Heuristic#Capture_History)
- [MVV-LVA](https://www.chessprogramming.org/MVV-LVA)
- [Move Ordering § Captures](https://www.chessprogramming.org/Move_Ordering#Captures)
- There is no standalone "Capture History" page; [/Capture_History](https://www.chessprogramming.org/Capture_History) returns 404.

**mudskipper** at `afdd37b`
- [src/search/ordering.rs](../../src/search/ordering.rs)
- [src/search/history.rs](../../src/search/history.rs)
- [src/search/worker.rs](../../src/search/worker.rs)
- [src/chess/board/see.rs](../../src/chess/board/see.rs)
