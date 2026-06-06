pub use ferrolith_macro::grammar;

pub mod action {
    pub use ferrolith_core::table::Action;
}

pub enum ParseError<T> {
    UnexpectedToken(T),
    UnexpectedEof,
}

impl<T> std::fmt::Debug for ParseError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken(_) => write!(f, "ParseError::UnexpectedToken(..)"),
            ParseError::UnexpectedEof => write!(f, "ParseError::UnexpectedEof"),
        }
    }
}
