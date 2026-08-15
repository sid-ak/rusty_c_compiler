//! The lexer: source bytes in, a token stream out.
//!
//! The scanner reads `&[u8]` rather than `&str`. A C file is not guaranteed to be valid UTF-8, and
//! this pass is a fuzz target handed arbitrary input, so malformed bytes have to become a
//! diagnostic rather than a decoding failure before the compiler starts.

pub mod token;

pub use token::{Keyword, Token, TokenKind};
