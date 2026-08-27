#!/usr/bin/env bash
# Builds the scratch repository the manual's screenshots are taken
# against.
#
#   ./demo-repo.sh /tmp/gambit
#
# It exists so the pictures can be retaken — on either platform, in
# either appearance — and still show the same code. A chess move
# generator, chosen because it is small, obviously synthetic, and
# unrelated to anything anyone works on.
set -euo pipefail

TARGET="${1:-/tmp/gambit}"
rm -rf "$TARGET"
mkdir -p "$TARGET/src"
cd "$TARGET"

cat > Cargo.toml <<'EOF'
[package]
name = "gambit"
version = "0.1.0"
edition = "2021"
EOF

cat > src/lib.rs <<'EOF'
pub mod board;
pub mod moves;
pub mod notation;
EOF

# --- the committed state -------------------------------------------------

cat > src/board.rs <<'EOF'
//! The board, and the squares pieces stand on.

/// A square in algebraic notation, `a1` through `h8`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Square(u8);

impl Square {
    pub fn file(self) -> u8 {
        self.0 % 8
    }

    pub fn rank(self) -> u8 {
        self.0 / 8
    }

    /// The square `files` right and `ranks` up, if it is on the board.
    pub fn offset(self, files: i8, ranks: i8) -> Option<Square> {
        let file = self.file() as i8 + files;
        let rank = self.rank() as i8 + ranks;
        Some(Square((rank * 8 + file) as u8))
    }
}
EOF

cat > src/moves.rs <<'EOF'
//! Move generation, one piece at a time.

use crate::board::Square;

/// The eight jumps a knight can make, as (files, ranks).
const KNIGHT: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

/// Every square a knight on `from` can reach.
pub fn knight_moves(from: Square) -> Vec<Square> {
    KNIGHT
        .iter()
        .filter_map(|(files, ranks)| from.offset(*files, *ranks))
        .collect()
}
EOF

git init -q -b main
git config user.name "Ada Lovelace"
git config user.email "ada@example.com"
git add -A
git commit -q -m "A board and the knight's moves"

cat > src/notation.rs <<'EOF'
//! Reading and writing moves in algebraic notation.

use crate::board::Square;

/// The letters files are written with, from the queenside.
pub const FILES: [char; 8] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];

/// The piece letters, as they appear in a move.
pub const PIECES: [char; 5] = ['K', 'Q', 'R', 'B', 'N'];

/// Writes a square the way a scoresheet would: `e4`, `h8`.
pub fn write_square(square: Square) -> String {
    let file = FILES[square.file() as usize];
    let rank = square.rank() + 1;
    format!("{file}{rank}")
}

/// Reads a square, rejecting anything outside the board.
pub fn read_square(text: &str) -> Option<Square> {
    let mut characters = text.chars();
    let file = characters.next()?;
    let rank = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let file = FILES.iter().position(|candidate| *candidate == file)?;
    let rank = rank.to_digit(10)? as usize;
    if !(1..=8).contains(&rank) {
        return None;
    }
    Square::from_index(((rank - 1) * 8 + file) as u8)
}

/// True when the move text names a piece rather than a pawn.
pub fn names_piece(text: &str) -> bool {
    text.chars().next().is_some_and(|first| PIECES.contains(&first))
}

/// Strips the decorations a scoresheet adds: check, mate, annotations.
pub fn strip_decorations(text: &str) -> &str {
    text.trim_end_matches(['+', '#', '!', '?'])
}

/// Splits a move into its piece letter and the rest of the move.
pub fn split_piece(text: &str) -> (Option<char>, &str) {
    if names_piece(text) {
        let mut characters = text.chars();
        let piece = characters.next();
        (piece, characters.as_str())
    } else {
        (None, text)
    }
}

/// Castling, written the long way and the short way.
pub fn is_castling(text: &str) -> bool {
    matches!(strip_decorations(text), "O-O" | "O-O-O" | "0-0" | "0-0-0")
}
EOF

git add src/notation.rs
git commit -q -m "Algebraic notation"

# --- the working tree under review --------------------------------------

cat > src/board.rs <<'EOF'
//! The board, and the squares pieces stand on.

/// How many files (and ranks) a board has.
pub const WIDTH: i8 = 8;

/// A square in algebraic notation, `a1` through `h8`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Square(u8);

impl Square {
    pub fn file(self) -> u8 {
        self.0 % WIDTH as u8
    }

    pub fn rank(self) -> u8 {
        self.0 / WIDTH as u8
    }

    /// A square from its index, when the index is on the board.
    pub fn from_index(index: u8) -> Option<Square> {
        (index < 64).then_some(Square(index))
    }

    /// The square `files` right and `ranks` up, if it is on the board.
    ///
    /// Files wrap around the edge when they are only checked after the
    /// index is built, so both coordinates are validated first.
    pub fn offset(self, files: i8, ranks: i8) -> Option<Square> {
        let file = self.file() as i8 + files;
        let rank = self.rank() as i8 + ranks;
        if !(0..WIDTH).contains(&file) || !(0..WIDTH).contains(&rank) {
            return None;
        }
        Some(Square((rank * WIDTH + file) as u8))
    }
}
EOF

cat >> src/moves.rs <<'EOF'

/// The one step a king takes, in every direction it can take it.
const KING: [(i8, i8); 8] = [
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
];

/// Every square a king on `from` can reach, castling aside.
pub fn king_moves(from: Square) -> Vec<Square> {
    KING.iter()
        .filter_map(|(files, ranks)| from.offset(*files, *ranks))
        .collect()
}
EOF

python3 - "$TARGET/src/notation.rs" <<'PYEOF'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
old = """/// True when the move text names a piece rather than a pawn.
pub fn names_piece(text: &str) -> bool {
    text.chars().next().is_some_and(|first| PIECES.contains(&first))
}"""
new = """/// True when the move text names a piece rather than a pawn.
///
/// Castling starts with `O`, which is not a piece letter — and not a
/// pawn move either, so callers check `is_castling` first.
pub fn names_piece(text: &str) -> bool {
    let first = text.chars().next();
    first.is_some_and(|letter| PIECES.contains(&letter))
}"""
assert old in s
p.write_text(s.replace(old, new))
PYEOF

echo "demo repository at $TARGET"
git -C "$TARGET" diff --stat
