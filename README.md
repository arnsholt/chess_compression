# A chess compression library

This crate is a straight port of 
[the Lichess move compression code](https://github.com/lichess-org/compression/)
from Java to Rust, but with a slightly different API.  The Rust code passes 
the  same test corpus as the Java code, so it _should_ be  compatible with 
compressed data from that library, but this hasn't been explored beyond 
passing the test corpus.
