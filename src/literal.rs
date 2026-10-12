//! One literal as a statement carries it, read back: the bytes a binary
//! literal names, or a string literal's text with its escapes undone.

/// One literal read from a statement, in a dialect whose binary literal is
/// its own syntax rather than a string: `X'…'`, `0x…`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// The bytes a binary literal names.
    Bytes(Vec<u8>),
    /// A string literal's text, its escapes undone.
    Text(String),
}
