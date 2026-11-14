# Castling is modelled Chess960-natively

Castling rights are stored as the starting square of each castling rook, and a castling move is encoded internally as "king captures own rook". Standard chess is just one Chess960 starting position. We decided this up front because adding Chess960 later touches move generation, make/unmake, FEN parsing and UCI move I/O all at once, and DFRC openings are useful for SPRT. The UCI layer translates to and from the standard `e1g1` notation when `UCI_Chess960` is off.
