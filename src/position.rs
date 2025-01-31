//! TODO: Module docs.

use shakmaty::{
    Bitboard,
    Color::{Black, White},
    Piece,
    Role::*,
    Setup, Square,
};
use std::num::NonZero;

use crate::Error;

/// Compress a position.
pub fn compress(position: &Setup) -> Result<Vec<u8>, Error> {
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
                .ok_or(Error::SquareOffsetError(sq, offset))
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
                Ok::<_, Error>(piece_value(
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
pub fn decompress(mut bytes: &[u8]) -> Result<Setup, Error> {
    let occupied = Bitboard(u64::from_be_bytes(
        bytes
            .get(0..8)
            .ok_or(Error::MissingBytes)?
            .try_into()
            .unwrap(),
    ));
    let mut setup = Setup::empty();

    let mut i = 8;
    let mut byte = 0;
    let mut read_more = true;
    for square in occupied {
        let value = if read_more {
            byte = *bytes.get(i).ok_or(Error::MissingBytes)?;
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
                    .ok_or(Error::SquareOffsetError(square, offset))?,
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
