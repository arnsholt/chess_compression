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

use bitbit::{BitReader, BitWriter, MSB};
use itertools::Itertools;
use shakmaty::{Chess, Color, Move, Position, Role};
use std::fmt::Formatter;
use std::io::{Read, Write};

#[cfg(test)]
mod tests;

/* Public API: */

/// Errors that can occur when decompressing or compressing moves.
#[derive(Debug)]
pub enum Error {
    /// I/O error from the underlying [`BitReader`] or [`BitWriter`].
    IO(std::io::Error),
    /// Error when applying a move to a position during compression or
    /// decompression.
    Chess(shakmaty::PlayError<Chess>),
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

/// Compress a sequence of moves from the starting position.
pub fn compress(moves: &[Move]) -> Result<Vec<u8>, Error> {
    compress_from_position(moves, Chess::default())
}

/// Compress a sequence of moves from a given position.
pub fn compress_from_position(moves: &[Move], position: Chess) -> Result<Vec<u8>, Error> {
    let mut position = position;
    let mut output = Vec::new();
    let mut writer = BitWriter::new(&mut output);
    for m in moves {
        write_move(m, &position, &mut writer)?;
        position = position.play(m).map_err(Error::Chess)?;
    }
    writer.pad_to_byte().map_err(Error::IO)?;
    Ok(output)
}

/// Decompress a given number of moves from the starting position.
pub fn decompress<R: Read>(input: R, plies: i32) -> Result<Vec<Move>, Error> {
    decompress_from_position(input, plies, Chess::default())
}

/// Decompress a given number of moves from a given position.
pub fn decompress_from_position<R: Read>(input: R, plies: i32, position: Chess) -> Result<Vec<Move>, Error> {
    let mut reader = BitReader::<_, MSB>::new(input);
    let mut position = position;
    let mut moves = Vec::new();

    for _i in 0..plies {
        let m = read_move(&mut reader, &position)?;
        position = position.play(&m).map_err(Error::Chess)?;
        moves.push(m);
    }

    Ok(moves)
}

/// Low-level function writing a single compressed move to a [`BitWriter`].
///
/// Remember that the writer buffers partially-written bytes, so your output
/// will be truncated if you forget to call [`BitWriter::pad_to_byte`] after
/// you have written all your moves.
pub fn write_move<W: Write>(m: &Move, position: &Chess, writer: &mut BitWriter<W>) -> Result<(), Error> {
    let moves = sorted_moves(position);
    let idx = moves.into_iter().position(|r| r == *m);
    if let Some(idx) = idx {
        write(idx as u8, writer)
    }
    else {
        Err(Error::MoveNotFound)
    }
}

/// TODO: Documentation
pub fn read_move<R: Read>(reader: &mut BitReader<R, MSB>, position: &Chess) -> Result<Move, Error> {
    let idx = read(reader)?;
    let moves = sorted_moves(position);
    Ok(moves[idx as usize].clone())
}

/* Internal API implementing the compression: */

struct Symbol (u32, u8);

enum Node {
    Interior { zero: Box<Node>, one: Box<Node> },
    Leaf(u8),
}

const PSQT: [[i32; 64]; 6] = [
       [   0,  0,  0,  0,  0,  0,  0,  0,
           50, 50, 50, 50, 50, 50, 50, 50,
           10, 10, 20, 30, 30, 20, 10, 10,
            5,  5, 10, 25, 25, 10,  5,  5,
            0,  0,  0, 20, 21,  0,  0,  0,
            5, -5,-10,  0,  0,-10, -5,  5,
            5, 10, 10,-31,-31, 10, 10,  5,
            0,  0,  0,  0,  0,  0,  0,  0 ],

        [ -50,-40,-30,-30,-30,-30,-40,-50,
          -40,-20,  0,  0,  0,  0,-20,-40,
          -30,  0, 10, 15, 15, 10,  0,-30,
          -30,  5, 15, 20, 20, 15,  5,-30,
          -30,  0, 15, 20, 20, 15,  0,-30,
          -30,  5, 10, 15, 15, 11,  5,-30,
          -40,-20,  0,  5,  5,  0,-20,-40,
          -50,-40,-30,-30,-30,-30,-40,-50 ],

        [ -20,-10,-10,-10,-10,-10,-10,-20,
          -10,  0,  0,  0,  0,  0,  0,-10,
          -10,  0,  5, 10, 10,  5,  0,-10,
          -10,  5,  5, 10, 10,  5,  5,-10,
          -10,  0, 10, 10, 10, 10,  0,-10,
          -10, 10, 10, 10, 10, 10, 10,-10,
          -10,  5,  0,  0,  0,  0,  5,-10,
          -20,-10,-10,-10,-10,-10,-10,-20 ],

        [   0,  0,  0,  0,  0,  0,  0,  0,
            5, 10, 10, 10, 10, 10, 10,  5,
           -5,  0,  0,  0,  0,  0,  0, -5,
           -5,  0,  0,  0,  0,  0,  0, -5,
           -5,  0,  0,  0,  0,  0,  0, -5,
           -5,  0,  0,  0,  0,  0,  0, -5,
           -5,  0,  0,  0,  0,  0,  0, -5,
            0,  0,  0,  5,  5,  0,  0,  0 ],

        [ -20,-10,-10, -5, -5,-10,-10,-20,
          -10,  0,  0,  0,  0,  0,  0,-10,
          -10,  0,  5,  5,  5,  5,  0,-10,
           -5,  0,  5,  5,  5,  5,  0, -5,
            0,  0,  5,  5,  5,  5,  0, -5,
          -10,  5,  5,  5,  5,  5,  0,-10,
          -10,  0,  5,  0,  0,  0,  0,-10,
          -20,-10,-10, -5, -5,-10,-10,-20 ],

        [ -30,-40,-40,-50,-50,-40,-40,-30,
          -30,-40,-40,-50,-50,-40,-40,-30,
          -30,-40,-40,-50,-50,-40,-40,-30,
          -30,-40,-40,-50,-50,-40,-40,-30,
          -20,-30,-30,-40,-40,-30,-30,-20,
          -10,-20,-20,-20,-20,-20,-20,-10,
           20, 20,  0,  0,  0,  0, 20, 20,
            0, 30, 10,  0,  0, 10, 30,  0 ]
    ];

const CODES: [Symbol; 256] = [
    Symbol(0b00, 2), // 0: 225883932
    Symbol(0b100, 3), // 1: 134956126
    Symbol(0b1101, 4), // 2: 89041269
    Symbol(0b1010, 4), // 3: 69386238
    Symbol(0b0101, 4), // 4: 57040790
    Symbol(0b11101, 5), // 5: 44974559
    Symbol(0b10111, 5), // 6: 36547155
    Symbol(0b01110, 5), // 7: 31624920
    Symbol(0b01100, 5), // 8: 28432772
    Symbol(0b01000, 5), // 9: 26540493
    Symbol(0b111101, 6), // 10: 24484873
    Symbol(0b111001, 6), // 11: 23058034
    Symbol(0b111100, 6), // 12: 23535272
    Symbol(0b110011, 6), // 13: 20482457
    Symbol(0b110010, 6), // 14: 20450172
    Symbol(0b110000, 6), // 15: 18316057
    Symbol(0b101101, 6), // 16: 17214833
    Symbol(0b101100, 6), // 17: 16964761
    Symbol(0b011111, 6), // 18: 16530028
    Symbol(0b011011, 6), // 19: 15369510
    Symbol(0b010011, 6), // 20: 14178440
    Symbol(0b011010, 6), // 21: 14275714
    Symbol(0b1111111, 7), // 22: 13353306
    Symbol(0b1111101, 7), // 23: 12829602
    Symbol(0b1111110, 7), // 24: 13102592
    Symbol(0b1111100, 7), // 25: 11932647
    Symbol(0b1110000, 7), // 26: 10608657
    Symbol(0b1100011, 7), // 27: 10142459
    Symbol(0b0111101, 7), // 28: 8294594
    Symbol(0b0100101, 7), // 29: 7337490
    Symbol(0b0100100, 7), // 30: 6337744
    Symbol(0b11100010, 8), // 31: 5380717
    Symbol(0b11000101, 8), // 32: 4560556
    Symbol(0b01111001, 8), // 33: 3913313
    Symbol(0b111000111, 9), // 34: 3038767
    Symbol(0b110001001, 9), // 35: 2480514
    Symbol(0b011110001, 9), // 36: 1951026
    Symbol(0b011110000, 9), // 37: 1521451
    Symbol(0b1110001100, 10), // 38: 1183252
    Symbol(0b1100010000, 10), // 39: 938708
    Symbol(0b11100011010, 11), // 40: 673339
    Symbol(0b11000100010, 11), // 41: 513153
    Symbol(0b111000110110, 12), // 42: 377299
    Symbol(0b110001000110, 12), // 43: 276996
    Symbol(0b1110001101110, 13), // 44: 199682
    Symbol(0b1100010001110, 13), // 45: 144602
    Symbol(0b11100011011110, 14), // 46: 103313
    Symbol(0b11000100011110, 14), // 47: 73046
    Symbol(0b111000110111110, 15), // 48: 52339
    Symbol(0b110001000111110, 15), // 49: 36779
    Symbol(0b1110001101111110, 16), // 50: 26341
    Symbol(0b1100010001111110, 16), // 51: 18719
    Symbol(0b11000100011111111, 17), // 52: 13225
    Symbol(0b111000110111111111, 18), // 53: 9392
    Symbol(0b111000110111111101, 18), // 54: 6945
    Symbol(0b110001000111111100, 18), // 55: 4893
    Symbol(0b1110001101111111100, 19), // 56: 3698
    Symbol(0b1100010001111111011, 19), // 57: 2763
    Symbol(0b11100011011111111011, 20), // 58: 2114
    Symbol(0b11100011011111110010, 20), // 59: 1631
    Symbol(0b11100011011111110000, 20), // 60: 1380
    Symbol(0b111000110111111110101, 21), // 61: 1090
    Symbol(0b111000110111111100110, 21), // 62: 887
    Symbol(0b111000110111111100010, 21), // 63: 715
    Symbol(0b110001000111111101001, 21), // 64: 590
    Symbol(0b110001000111111101000, 21), // 65: 549
    Symbol(0b1110001101111111101000, 22), // 66: 477
    Symbol(0b1110001101111111000110, 22), // 67: 388
    Symbol(0b1100010001111111010111, 22), // 68: 351
    Symbol(0b1100010001111111010101, 22), // 69: 319
    Symbol(0b11100011011111111010011, 23), // 70: 262
    Symbol(0b11100011011111110011110, 23), // 71: 236
    Symbol(0b11100011011111110001110, 23), // 72: 200
    Symbol(0b11100011011111110001111, 23), // 73: 210
    Symbol(0b11000100011111110101100, 23), // 74: 153
    Symbol(0b111000110111111100111011, 24), // 75: 117
    Symbol(0b111000110111111110100100, 24), // 76: 121
    Symbol(0b111000110111111100111111, 24), // 77: 121
    Symbol(0b111000110111111100111010, 24), // 78: 115
    Symbol(0b110001000111111101011011, 24), // 79: 95
    Symbol(0b110001000111111101010011, 24), // 80: 75
    Symbol(0b110001000111111101010001, 24), // 81: 67
    Symbol(0b1110001101111111001110011, 25), // 82: 55
    Symbol(0b1110001101111111001110001, 25), // 83: 50
    Symbol(0b1110001101111111001110010, 25), // 84: 55
    Symbol(0b1100010001111111010100101, 25), // 85: 33
    Symbol(0b1100010001111111010110100, 25), // 86: 33
    Symbol(0b1100010001111111010100001, 25), // 87: 30
    Symbol(0b11100011011111110011111011, 26), // 88: 32
    Symbol(0b11100011011111110011111001, 26), // 89: 28
    Symbol(0b11100011011111110011111010, 26), // 90: 29
    Symbol(0b11100011011111110011111000, 26), // 91: 27
    Symbol(0b11000100011111110101101011, 26), // 92: 21
    Symbol(0b111000110111111110100101111, 27), // 93: 15
    Symbol(0b110001000111111101011010100, 27), // 94: 9
    Symbol(0b110001000111111101011010101, 27), // 95: 10
    Symbol(0b111000110111111100111000010, 27), // 96: 12
    Symbol(0b111000110111111100111000011, 27), // 97: 12
    Symbol(0b110001000111111101010010011, 27), // 98: 8
    Symbol(0b1110001101111111101001010011, 28), // 99: 7
    Symbol(0b1100010001111111010100100101, 28), // 100: 2
    Symbol(0b1110001101111111001110000011, 28), // 101: 4
    Symbol(0b1110001101111111001110000010, 28), // 102: 5
    Symbol(0b1110001101111111001110000000, 28), // 103: 5
    Symbol(0b11100011011111110011100000010, 29), // 104
    Symbol(0b11000100011111110101000001001, 29), // 105: 5
    Symbol(0b11100011011111110011100000011, 29), // 106: 1
    Symbol(0b11000100011111110101000001000, 29), // 107: 1
    Symbol(0b11000100011111110101000000011, 29), // 108
    Symbol(0b110001000111111101010000011110, 30), // 109: 1
    Symbol(0b111000110111111110100101100110, 30), // 110: 2
    Symbol(0b111000110111111110100101010111, 30), // 111: 1
    Symbol(0b110001000111111101010000001101, 30), // 112: 1
    Symbol(0b111000110111111110100101100010, 30), // 113
    Symbol(0b110001000111111101010000001000, 30), // 114
    Symbol(0b110001000111111101010000000101, 30), // 115: 1
    Symbol(0b110001000111111101010000000000, 30), // 116
    Symbol(0b110001000111111101010000001010, 30), // 117
    Symbol(0b110001000111111101010010001101, 30), // 118
    Symbol(0b110001000111111101010010010011, 30), // 119
    Symbol(0b110001000111111101010010010010, 30), // 120
    Symbol(0b110001000111111101010010010001, 30), // 121
    Symbol(0b110001000111111101010010010000, 30), // 122
    Symbol(0b110001000111111101010010001011, 30), // 123
    Symbol(0b110001000111111101010010001010, 30), // 124
    Symbol(0b110001000111111101010010001001, 30), // 125
    Symbol(0b110001000111111101010010001000, 30), // 126
    Symbol(0b110001000111111101010010000111, 30), // 127
    Symbol(0b110001000111111101010010000110, 30), // 128
    Symbol(0b110001000111111101010010000011, 30), // 129
    Symbol(0b110001000111111101010010000010, 30), // 130
    Symbol(0b110001000111111101010000011011, 30), // 131
    Symbol(0b110001000111111101010000011010, 30), // 132
    Symbol(0b110001000111111101010000011001, 30), // 133
    Symbol(0b110001000111111101010000011000, 30), // 134
    Symbol(0b110001000111111101010000010101, 30), // 135
    Symbol(0b110001000111111101010000010100, 30), // 136
    Symbol(0b110001000111111101010010000101, 30), // 137
    Symbol(0b110001000111111101010010000100, 30), // 138
    Symbol(0b110001000111111101010000011111, 30), // 139
    Symbol(0b110001000111111101010000011101, 30), // 140
    Symbol(0b110001000111111101010000011100, 30), // 141
    Symbol(0b110001000111111101010010000001, 30), // 142
    Symbol(0b110001000111111101010010000000, 30), // 143
    Symbol(0b110001000111111101010000001111, 30), // 144
    Symbol(0b110001000111111101010000001110, 30), // 145
    Symbol(0b110001000111111101010000001100, 30), // 146
    Symbol(0b110001000111111101010000010111, 30), // 147
    Symbol(0b110001000111111101010000010110, 30), // 148
    Symbol(0b110001000111111101010000001001, 30), // 149
    Symbol(0b110001000111111101010000000100, 30), // 150
    Symbol(0b110001000111111101010000000011, 30), // 151
    Symbol(0b110001000111111101010000000010, 30), // 152
    Symbol(0b110001000111111101010000000001, 30), // 153
    Symbol(0b110001000111111101010000001011, 30), // 154
    Symbol(0b110001000111111101010010001111, 30), // 155
    Symbol(0b110001000111111101010010001110, 30), // 156
    Symbol(0b110001000111111101010010001100, 30), // 157
    Symbol(0b1110001101111111101001010111101, 31), // 158
    Symbol(0b1110001101111111101001010111111, 31), // 159
    Symbol(0b1110001101111111101001010100010, 31), // 160
    Symbol(0b1110001101111111101001011011111, 31), // 161
    Symbol(0b1110001101111111101001010100100, 31), // 162
    Symbol(0b1110001101111111101001010111001, 31), // 163
    Symbol(0b1110001101111111101001011011010, 31), // 164
    Symbol(0b1110001101111111101001011010010, 31), // 165
    Symbol(0b1110001101111111101001011010000, 31), // 166
    Symbol(0b1110001101111111101001010111010, 31), // 167
    Symbol(0b1110001101111111101001010001011, 31), // 168
    Symbol(0b1110001101111111101001010001010, 31), // 169
    Symbol(0b1110001101111111101001010001001, 31), // 170
    Symbol(0b1110001101111111101001010001000, 31), // 171
    Symbol(0b1110001101111111101001010000111, 31), // 172
    Symbol(0b1110001101111111101001010000110, 31), // 173
    Symbol(0b1110001101111111101001010000101, 31), // 174
    Symbol(0b1110001101111111101001010000100, 31), // 175
    Symbol(0b1110001101111111101001011010111, 31), // 176
    Symbol(0b1110001101111111101001011010110, 31), // 177
    Symbol(0b1110001101111111101001011010101, 31), // 178
    Symbol(0b1110001101111111101001011010100, 31), // 179
    Symbol(0b1110001101111111101001010110111, 31), // 180
    Symbol(0b1110001101111111101001010110110, 31), // 181
    Symbol(0b1110001101111111101001010010101, 31), // 182
    Symbol(0b1110001101111111101001010010100, 31), // 183
    Symbol(0b1110001101111111101001010110101, 31), // 184
    Symbol(0b1110001101111111101001010110100, 31), // 185
    Symbol(0b1110001101111111101001010010111, 31), // 186
    Symbol(0b1110001101111111101001010010110, 31), // 187
    Symbol(0b1110001101111111101001010110001, 31), // 188
    Symbol(0b1110001101111111101001010110000, 31), // 189
    Symbol(0b1110001101111111101001010010011, 31), // 190
    Symbol(0b1110001101111111101001010010010, 31), // 191
    Symbol(0b1110001101111111101001011101101, 31), // 192
    Symbol(0b1110001101111111101001011101100, 31), // 193
    Symbol(0b1110001101111111101001011101011, 31), // 194
    Symbol(0b1110001101111111101001011101010, 31), // 195
    Symbol(0b1110001101111111101001011100111, 31), // 196
    Symbol(0b1110001101111111101001011100110, 31), // 197
    Symbol(0b1110001101111111101001010010001, 31), // 198
    Symbol(0b1110001101111111101001010010000, 31), // 199
    Symbol(0b1110001101111111101001011100011, 31), // 200
    Symbol(0b1110001101111111101001011100010, 31), // 201
    Symbol(0b1110001101111111101001011100001, 31), // 202
    Symbol(0b1110001101111111101001011100000, 31), // 203
    Symbol(0b1110001101111111101001011101001, 31), // 204
    Symbol(0b1110001101111111101001011101000, 31), // 205
    Symbol(0b1110001101111111101001010001111, 31), // 206
    Symbol(0b1110001101111111101001010001110, 31), // 207
    Symbol(0b1110001101111111101001010000011, 31), // 208
    Symbol(0b1110001101111111101001010000010, 31), // 209
    Symbol(0b1110001101111111101001010001101, 31), // 210
    Symbol(0b1110001101111111101001010001100, 31), // 211
    Symbol(0b1110001101111111101001011001111, 31), // 212
    Symbol(0b1110001101111111101001011001110, 31), // 213
    Symbol(0b1110001101111111101001010000001, 31), // 214
    Symbol(0b1110001101111111101001010000000, 31), // 215
    Symbol(0b1110001101111111101001011011001, 31), // 216
    Symbol(0b1110001101111111101001011011000, 31), // 217
    Symbol(0b1110001101111111101001011100101, 31), // 218
    Symbol(0b1110001101111111101001011100100, 31), // 219
    Symbol(0b1110001101111111101001010101101, 31), // 220
    Symbol(0b1110001101111111101001010101100, 31), // 221
    Symbol(0b1110001101111111101001010110011, 31), // 222
    Symbol(0b1110001101111111101001010110010, 31), // 223
    Symbol(0b1110001101111111101001010101001, 31), // 224
    Symbol(0b1110001101111111101001010101000, 31), // 225
    Symbol(0b1110001101111111101001011101111, 31), // 226
    Symbol(0b1110001101111111101001011101110, 31), // 227
    Symbol(0b1110001101111111101001011001011, 31), // 228
    Symbol(0b1110001101111111101001011001010, 31), // 229
    Symbol(0b1110001101111111101001011000011, 31), // 230
    Symbol(0b1110001101111111101001011000010, 31), // 231
    Symbol(0b1110001101111111101001010101011, 31), // 232
    Symbol(0b1110001101111111101001010101010, 31), // 233
    Symbol(0b1110001101111111101001011001001, 31), // 234
    Symbol(0b1110001101111111101001011001000, 31), // 235
    Symbol(0b1110001101111111101001011000111, 31), // 236
    Symbol(0b1110001101111111101001011000110, 31), // 237
    Symbol(0b1110001101111111101001011000001, 31), // 238
    Symbol(0b1110001101111111101001011000000, 31), // 239
    Symbol(0b1110001101111111101001010111100, 31), // 240
    Symbol(0b1110001101111111101001010100111, 31), // 241
    Symbol(0b1110001101111111101001010100110, 31), // 242
    Symbol(0b1110001101111111101001010111110, 31), // 243
    Symbol(0b1110001101111111101001010100011, 31), // 244
    Symbol(0b1110001101111111101001010100001, 31), // 245
    Symbol(0b1110001101111111101001010100000, 31), // 246
    Symbol(0b1110001101111111101001011011110, 31), // 247
    Symbol(0b1110001101111111101001010100101, 31), // 248
    Symbol(0b1110001101111111101001011011101, 31), // 249
    Symbol(0b1110001101111111101001011011100, 31), // 250
    Symbol(0b1110001101111111101001010111000, 31), // 251
    Symbol(0b1110001101111111101001011011011, 31), // 252
    Symbol(0b1110001101111111101001011010001, 31), // 253
    Symbol(0b1110001101111111101001011010011, 31), // 254
    Symbol(0b1110001101111111101001010111011, 31), // 255
];

lazy_static! {
    static ref ROOT: Node = build_tree(0, 0);
}

fn move_value(position: &Chess, m: &Move) -> i32 {
    let role_idx = usize::from(m.role()) - 1;
    let flip = position.turn() == Color::White;
    let to = m.to();
    /* XXX: Safe to unwrap here, since we only support Chess, but *will* be
     * None if we ever support crazyhouse or other variants with drops: */
    let from = m.from().unwrap();
    let (to_idx, from_idx): (usize, usize) = if flip {
        (to.flip_vertical().into(), from.flip_vertical().into())
    }
    else {
        (to.into(), from.into())
    };
    PSQT[role_idx][to_idx] - PSQT[role_idx][from_idx]
}

fn move_score(m: &Move, position: &Chess) -> i32 {
    let defending_pawns =
        shakmaty::attacks::pawn_attacks(position.turn(), m.to())
        & position.their(Role::Pawn);
    let defending_pawn_score = if defending_pawns.0 == 0  {
        6
    }
    else {
        6 - i32::from(m.role())
    };
    let move_value = move_value(position, m);
    let score = (if let Some(promoted) = m.promotion() { (i32::from(promoted) - 1) << 26 } else { 0 })
        + (if m.is_capture() { 1 << 25 } else { 0 })
        + (defending_pawn_score << 22)
        + ((512 + move_value) << 12)
        + (i32::from(m.to()) << 6)
        /* XXX: Safe to unwrap here, since we only support Chess, but *will*
         * be None if we ever support crazyhouse or other variants with drops: */
        + i32::from(m.from().unwrap());
    -score
}

fn sorted_moves(position: &Chess) -> Vec<Move> {
    position.legal_moves().into_iter().sorted_by_key(|m| move_score(m, position)).collect()
}

fn write<W: Write>(value: u8, writer: &mut BitWriter<W>) -> Result<(), Error> {
    let code = &CODES[value as usize];
    writer.write_bits(code.0, code.1 as usize).map_err(Error::IO)
}

fn read<R: Read>(reader: &mut BitReader<R, MSB>) -> Result<u8, Error> {
    let mut node: &Node = &*ROOT;
    loop {
        match node {
            Node::Interior { zero, one } => {
                node = if reader.read_bit().map_err(Error::IO)? {
                    one
                }
                else {
                    zero
                };
            },
            Node::Leaf(n) => return Ok(*n),
        }
    }
}

fn build_tree(code: u32, bits: u8) -> Node {
    for i in 0..=255 {
        let idx = i as usize;
        if CODES[idx].0 == code && CODES[idx].1 == bits {
            return Node::Leaf(i)
        }
    }

    Node::Interior {
        zero: Box::new(build_tree(code << 1, bits + 1)),
        one: Box::new(build_tree((code << 1) | 1, bits + 1)),
    }
}
