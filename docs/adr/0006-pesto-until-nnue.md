# The evaluation stays at PeSTO until NNUE replaces it

No hand-crafted evaluation terms (king safety, mobility, pawn structure, passed pawns and the like) are added. The evaluation stays the PeSTO tapered piece-square tables until an NNUE replaces it, which remains the long-term goal (ADR-0004). Effort goes into search instead. At this stage a search feature gains far more Elo per SPRT than an evaluation term. It also keeps its value once NNUE arrives, and the first network's training data will come from the engine's own search, so a stronger search makes better data.

Interim hand-crafted terms were considered and rejected, even though training a first network may be a long way off. PeSTO's values were fitted as a set, so terms added on top need a tuner and many SPRTs to avoid double-counting what the tables already encode. All of that work is thrown away the day NNUE lands.
