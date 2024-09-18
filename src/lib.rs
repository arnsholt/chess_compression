//! A library for compressing chess moves. This is a straight Rust port of a
//! [Java original](https://github.com/lichess-org/compression/) made by the
//! Lichess project, but with a slightly different user-facing API.
//!
//! Note that when decompressing, you need to know how many plies you want to
//! decompress. This is because a given move sequence is not guaranteed to
//! fill the last byte exactly. In this case, any trailing bits in the input
//! would cause havoc if we didn't know how many elements to decompress.

#[macro_use]
extern crate lazy_static;

use shakmaty::Chess;
use std::fmt::Formatter;

mod moves;
#[cfg(test)]
mod tests;

pub use moves::{
    compress, compress_from_position, decompress, decompress_from_position, read_move, write_move,
};
pub use position::{compress_position, decompress_position};

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
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IO(e) => Some(e),
            Self::Chess(e) => Some(e),
            Self::MoveNotFound => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(e) => write!(f, "IO error: {}", e),
            Self::Chess(e) => write!(f, "Chess error: {}", e),
            Self::MoveNotFound => write!(f, "Move not found in sorted move list"),
        }
    }
}
