//! A library for compressing chess moves and positions. The code is straight
//!  ports of [Java](https://github.com/lichess-org/compression/) and
//! [Scala](https://lichess.org/@/revoof/blog/adapting-nnue-pytorchs-binary-position-format-for-lichess/cpeeAMeY)
//! originals made by the Lichess project, with some tweaks to the API. The
//! code is split into two modules, one for compressing moves and one for
//! positions.

#[macro_use]
extern crate lazy_static;

use shakmaty::{Chess, Square};
use std::fmt::Formatter;

pub mod moves;
pub mod position;
#[cfg(test)]
mod tests;

/// Errors that can occur when decompressing or compressing moves.
#[derive(Debug)]
pub enum Error {
    /// I/O error from the underlying [`BitReader`](bitbit::BitReader) or
    /// [`BitWriter`](bitbit::BitWriter).
    IO(std::io::Error),
    /// Error when applying a move to a position during compression or
    /// decompression.
    Chess(Box<shakmaty::PlayError<Chess>>),
    /// Failure to find the move to compress in the list of legal moves in the
    /// target position.
    MoveNotFound,
    /// Not enough bytes to decompress a position.
    MissingBytes,
    /// Returned if square offsetting via [`Square::offset`](Square::offset)
    /// fails. This should never happen.
    SquareOffsetError(Square, i32),
    /// Error while reading a LEB128 encoded value.
    Leb128(leb128::read::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IO(e) => Some(e),
            Self::Chess(e) => Some(e),
            _ => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(e) => write!(f, "IO error: {}", e),
            Self::Chess(e) => write!(f, "Chess error: {}", e),
            Self::MoveNotFound => write!(f, "Move not found in sorted move list"),
            Self::MissingBytes => write!(f, "Not enough bytes in data to decompress position"),
            Self::SquareOffsetError(square, offset) => {
                write!(f, "Failed to offset square {square} by {offset}")
            }
            Self::Leb128(e) => write!(f, "Leb128 error: {}", e),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IO(value)
    }
}

impl From<leb128::read::Error> for Error {
    fn from(value: leb128::read::Error) -> Self {
        Self::Leb128(value)
    }
}
