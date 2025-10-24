//! Functions for compressing and decompressing chess positions.
//!
//! Our API works in terms of shakmaty's [`Setup`] type, a not necessarily
//! legal position, leaving the handling of illegal positions up to
//! downstream code. For a detailed description of the compression algorithm,
//! see the [blog post] and [code], but in brief the position is compressed to
//! a variable number of bytes:
//!
//! - A 64-bit BE int encoding which squares are occupied
//! - A sequence of bytes encoding the pieces on squares, two per byte.
//!   Special values encode castling rights, en passant and side to move.
//! - Optionally, LEB128-encoded halfmove clock and number of plies played
//!
//! [blog post]: https://lichess.org/@/revoof/blog/adapting-nnue-pytorchs-binary-position-format-for-lichess/cpeeAMeY
//! [code]: https://github.com/lichess-org/scalachess/blob/master/core/src/main/scala/format/BinaryFen.scala

use shakmaty::{
    Bitboard,
    Color::{Black, White},
    Piece,
    Role::*,
    Setup, Square,
};
use std::fmt::{Display, Formatter};
use std::num::NonZero;

/// Errors that can occur while compressing a position.
#[derive(Debug)]
pub enum CompressError {
    /// I/O error from the target data sink.
    IO(std::io::Error),
    /// Attempt to offset a square out of the chess board.
    SquareOffset(Square, i32),
}

impl From<std::io::Error> for CompressError {
    fn from(value: std::io::Error) -> Self {
        Self::IO(value)
    }
}

impl std::error::Error for CompressError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::IO(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

impl Display for CompressError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressError::IO(e) => write!(f, "IO error: {e}"),
            CompressError::SquareOffset(sq, i) => {
                write!(f, "Attempted to offset {sq} by {i} out of the board")
            }
        }
    }
}

/// Errors that can occur while decompressing a position.
#[derive(Debug)]
pub enum DecompressError {
    /// Premature end of input.
    MissingBytes,
    /// Attempt to offset a square out of the chess board.
    SquareOffset(Square, i32),
    /// Error while reading a LEB128 encoded value.
    Leb128(leb128::read::Error),
}

impl From<leb128::read::Error> for DecompressError {
    fn from(value: leb128::read::Error) -> Self {
        Self::Leb128(value)
    }
}

impl std::error::Error for DecompressError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Leb128(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

impl Display for DecompressError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DecompressError::MissingBytes => write!(f, "Missing input bytes to decompress"),
            DecompressError::SquareOffset(sq, i) => {
                write!(f, "Attempted to offset {sq} by {i} out of the board")
            }
            DecompressError::Leb128(e) => write!(f, "Leb128 error: {e}"),
        }
    }
}

/// Compress a position.
pub fn compress(position: &Setup) -> Result<Vec<u8>, CompressError> {
    let mut result = Vec::new();

    let board = &position.board;
    let occupied = board.occupied();
    result.extend(occupied.0.to_be_bytes());

    let pawn_pushed_to = position
        .ep_square
        .map(|sq| {
            // If it's black to play, *white* just pushed their pawn, so we
            // offset backwards.
            let offset = if position.turn == Black { 8 } else { -8 };
            sq.offset(offset)
                .map(Square::into)
                .ok_or(CompressError::SquareOffset(sq, offset))
        })
        .transpose()?
        .unwrap_or(Bitboard::EMPTY);

    let mut board_iter = board.clone().into_iter();
    /* We iterate over the occupancy of the board two-by-two, so that we can
     * fill each byte in the output with a single iteration of the loop. */
    while let Some(((sq, piece), maybe_pair)) = board_iter.next().map(|v| (v, board_iter.next())) {
        let black_turn = position.turn == Black;
        let lower_half = piece_value(
            piece,
            sq,
            black_turn,
            position.castling_rights,
            pawn_pushed_to,
        );
        let upper_half = maybe_pair
            .map(|(sq, piece)| {
                Ok::<_, CompressError>(piece_value(
                    piece,
                    sq,
                    black_turn,
                    position.castling_rights,
                    pawn_pushed_to,
                ))
            })
            .transpose()?
            .unwrap_or(0);
        result.push((upper_half << 4) | lower_half);
    }

    let ply = (position.fullmoves.get() - 1) * 2 + if position.turn == Black { 1 } else { 0 };
    let halfmoves = position.halfmoves;
    let broken_turn = position.turn == Black && position.board.king_of(Black).is_none();

    if halfmoves > 0 || ply > 1 || broken_turn {
        leb128::write::unsigned(&mut result, halfmoves as u64)?;
    }

    if ply > 1 || broken_turn {
        leb128::write::unsigned(&mut result, ply as u64)?;
    }

    Ok(result)
}

fn piece_value(
    piece: Piece,
    square: Square,
    black_turn: bool,
    unmoved_rooks: Bitboard,
    pawn_pushed_to: Bitboard,
) -> u8 {
    match piece {
        Piece {
            role: Pawn,
            color: _,
        } if pawn_pushed_to.contains(square) => 12,
        Piece {
            color: White,
            role: Pawn,
        } => 0,
        Piece {
            color: Black,
            role: Pawn,
        } => 1,
        Piece {
            color: White,
            role: Knight,
        } => 2,
        Piece {
            color: Black,
            role: Knight,
        } => 3,
        Piece {
            color: White,
            role: Bishop,
        } => 4,
        Piece {
            color: Black,
            role: Bishop,
        } => 5,
        Piece {
            color: White,
            role: Rook,
        } => {
            if unmoved_rooks.contains(square) {
                13
            } else {
                6
            }
        }
        Piece {
            color: Black,
            role: Rook,
        } => {
            if unmoved_rooks.contains(square) {
                14
            } else {
                7
            }
        }
        Piece {
            color: White,
            role: Queen,
        } => 8,
        Piece {
            color: Black,
            role: Queen,
        } => 9,
        Piece {
            color: White,
            role: King,
        } => 10,
        Piece {
            color: Black,
            role: King,
        } => {
            if black_turn {
                15
            } else {
                11
            }
        }
    }
}

/// Decompress a position.
pub fn decompress(mut bytes: &[u8]) -> Result<Setup, DecompressError> {
    let occupied = Bitboard(u64::from_be_bytes(
        bytes
            .get(0..8)
            .ok_or(DecompressError::MissingBytes)?
            .try_into()
            .unwrap(),
    ));
    let mut setup = Setup::empty();

    let mut i = 8;
    let mut byte = 0;
    let mut read_more = true;
    for square in occupied {
        let value = if read_more {
            byte = *bytes.get(i).ok_or(DecompressError::MissingBytes)?;
            i += 1;
            byte & 0x0f
        } else {
            (byte & 0xf0) >> 4
        };
        read_more = !read_more;

        let piece = piece_from_value(value, square);
        setup.board.set_piece_at(square, piece);
        if value == 12 {
            let offset = if Bitboard::SOUTH.contains(square) {
                -8
            } else {
                8
            };
            setup.ep_square = Some(
                square
                    .offset(offset)
                    .ok_or(DecompressError::SquareOffset(square, offset))?,
            );
        } else if value == 13 || value == 14 {
            setup.castling_rights |= square;
        } else if value == 15 {
            setup.turn = Black
        }
    }

    bytes = &bytes[i..];
    if !bytes.is_empty() {
        setup.halfmoves = leb128::read::unsigned(&mut bytes)? as u32;
    }
    if !bytes.is_empty() {
        let ply_count = leb128::read::unsigned(&mut bytes)? as u32;
        if (ply_count % 2) == 1 {
            setup.turn = Black;
        }
        let black_offset = if setup.turn == Black { 1 } else { 0 };
        setup.fullmoves = NonZero::new((ply_count - black_offset) / 2 + 1).unwrap();
    }

    Ok(setup)
}

fn piece_from_value(value: u8, square: Square) -> Piece {
    if value == 0 {
        Piece {
            color: White,
            role: Pawn,
        }
    } else if value == 1 {
        Piece {
            color: Black,
            role: Pawn,
        }
    } else if value == 2 {
        Piece {
            role: Knight,
            color: White,
        }
    } else if value == 3 {
        Piece {
            role: Knight,
            color: Black,
        }
    } else if value == 4 {
        Piece {
            role: Bishop,
            color: White,
        }
    } else if value == 5 {
        Piece {
            role: Bishop,
            color: Black,
        }
    } else if value == 6 || value == 13 {
        Piece {
            role: Rook,
            color: White,
        }
    } else if value == 7 || value == 14 {
        Piece {
            role: Rook,
            color: Black,
        }
    } else if value == 8 {
        Piece {
            role: Queen,
            color: White,
        }
    } else if value == 9 {
        Piece {
            role: Queen,
            color: Black,
        }
    } else if value == 10 {
        Piece {
            role: King,
            color: White,
        }
    } else if value == 11 || value == 15 {
        Piece {
            role: King,
            color: Black,
        }
    } else if value == 12 {
        Piece {
            role: Pawn,
            color: if Bitboard::SOUTH.contains(square) {
                White
            } else {
                Black
            },
        }
    } else {
        unreachable!()
    }
}
