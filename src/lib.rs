//! A library for compressing chess moves and positions. The code is straight
//!  ports of [Java](https://github.com/lichess-org/compression/) and
//! [Scala](https://lichess.org/@/revoof/blog/adapting-nnue-pytorchs-binary-position-format-for-lichess/cpeeAMeY)
//! originals made by the Lichess project, with some tweaks to the API. The
//! code is split into two modules, one for compressing moves and one for
//! positions.

#[macro_use]
extern crate lazy_static;

pub mod moves;
pub mod position;
#[cfg(test)]
mod tests;
